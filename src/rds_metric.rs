use std::borrow::Borrow;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use lazy_static::lazy_static;
use prometheus::register_gauge_vec;
use prometheus::{self, Gauge, GaugeVec, Histogram};
use crate::config::RDSInstance;
use async_trait::async_trait;
use aws_volume_limit_calculator;
use chrono::{Duration, Utc};
// use futures::future::join_all;
// use futures::try_join;
use tokio::task::{JoinHandle, JoinSet};

use rusoto_core::{Client, HttpClient, Region};
use rusoto_core::credential::{AwsCredentials, CredentialsError, ProvideAwsCredentials};
use rusoto_rds::{DBInstance, RdsClient, Rds, DescribeDBInstancesMessage, Filter};
use rusoto_cloudwatch::{CloudWatch, CloudWatchClient, Dimension, GetMetricDataInput, Metric, MetricDataQuery, MetricDataResult, MetricStat};
use rusoto_core::request::TlsError;
use tokio::try_join;
use crate::constants::StorageType;
use crate::read_config;

lazy_static! {
    pub static ref IOPS_LIMIT: GaugeVec = register_gauge_vec!("rds_iops_limit", "IOPS limit", &["instance"]).unwrap();
    pub static ref BURST_IOPS_LIMIT: GaugeVec = register_gauge_vec!("rds_burst_iops_limit", "Burst IOPS limit", &["instance"]).unwrap();
    pub static ref BURST_CREDIT_BALANCE: GaugeVec = register_gauge_vec!("burst_credit_balance", "Burst credit %", &["instance"]).unwrap();
    pub static ref WRITE_IOPS: GaugeVec = register_gauge_vec!("write_iops", "Write IOPS", &["instance"]).unwrap();
    pub static ref READ_IOPS: GaugeVec = register_gauge_vec!("read_iops", "Read IOPS", &["instance"]).unwrap();
    pub static ref TOTAL_IOPS: GaugeVec = register_gauge_vec!("total_iops", "Total IOPS", &["instance"]).unwrap();
}

#[derive(Debug)]
pub struct InstanceDetails {
    instance_identifier: String,
    storage_type: StorageType,
    storage_size_gb: i64,
    storage_provided_iops: Option<i64>,
}

impl InstanceDetails {
    pub fn calculate_limits(&self) -> Result<aws_volume_limit_calculator::Limit, Box<dyn Error>> {
        Ok(match self.storage_type {
            (StorageType::GP2) => {
                aws_volume_limit_calculator::calculate_gp2_limits(u32::try_from(self.storage_size_gb)?)?
            }
            (StorageType::GP3) => {
                aws_volume_limit_calculator::calculate_gp3_limits(u32::try_from(self.storage_size_gb)?, None, None)?
            }
            (StorageType::IO1) => {
                aws_volume_limit_calculator::calculate_io_limits(u32::try_from(self.storage_provided_iops.ok_or("No provided IOPS")?)?)?
            }
            _ => {
                aws_volume_limit_calculator::Limit::default()
            }
        })
    }
}

struct SimpleCredentials {
    public_key: String,
    secret_key: String,
}

#[async_trait]
impl ProvideAwsCredentials for SimpleCredentials {
    async fn credentials(&self) -> Result<AwsCredentials, CredentialsError> {
        Ok(AwsCredentials::new(self.public_key.clone(), self.secret_key.clone(), None, None))
    }
}

pub async fn get_instance_details(instance: RDSInstance) -> Result<InstanceDetails, String> {
    let provider = SimpleCredentials { public_key: instance.aws_access_key.clone(), secret_key: instance.aws_secret_key.clone() };
    let client = RdsClient::new_with(HttpClient::new().unwrap(), provider, Region::from_str(instance.region.as_str()).unwrap());
    let describe_input = DescribeDBInstancesMessage {
        db_instance_identifier: Option::from(instance.instance.clone()),
        filters: None,
        marker: None,
        max_records: None,
    };
    return match client.describe_db_instances(describe_input).await {
        Ok(output) => match output.db_instances {
            Some(db_instances) => {
                if db_instances.len() == 0 {
                    Err("No db instances")?
                } else {
                    let db_instance = db_instances.get(0).unwrap();
                    // Ok(db_instance.allocated_storage.clone().unwrap() as f64)
                    Ok(InstanceDetails {
                        instance_identifier: db_instance.db_instance_identifier.clone().unwrap(),
                        storage_type: StorageType::try_from_string(db_instance.storage_type.clone().unwrap().as_str()),
                        storage_size_gb: db_instance.allocated_storage.clone().unwrap(),
                        storage_provided_iops: db_instance.iops,
                    })
                }
            }
            None => {
                Err("No databases!")?
            }
        },
        Err(error) => {
            Err(format!("Error: {:?}", error))?
        }
    };
}

pub async fn get_instance_metric(instance: RDSInstance, namespace: &str, metric_name: &str, statistics: &str, dimensions: Vec<Dimension>) -> Result<Option<f64>, String> {
    let provider = SimpleCredentials { public_key: instance.aws_access_key.clone(), secret_key: instance.aws_secret_key.clone() };
    let client = CloudWatchClient::new_with(HttpClient::new().unwrap(), provider, Region::from_str(instance.region.as_str()).unwrap());
    let input = GetMetricDataInput {
        end_time: Utc::now().to_rfc3339(),
        start_time: (Utc::now() - Duration::minutes(1)).to_rfc3339(),
        label_options: None,
        max_datapoints: Option::from(1),
        metric_data_queries: vec![
            MetricDataQuery {
                expression: None,
                id: "some_id".to_string(),
                label: None,
                metric_stat: Option::from(MetricStat {
                    metric: Metric {
                        dimensions: Option::from(dimensions),
                        metric_name: Option::from(metric_name.to_string()),
                        namespace: Option::from(namespace.to_string()),
                    },
                    period: 300,
                    stat: statistics.to_string(),
                    unit: None,
                }),
                period: None,
                return_data: Option::from(true),
            }
        ],
        next_token: None,
        scan_by: None,
    };
    return match client.get_metric_data(input).await {
        Ok(output) => match output.metric_data_results {
            Some(results) => {
                if results.len() == 0 {
                    Ok(None)
                } else {
                    let result = results.get(0).unwrap();
                    let values = result.values.clone().unwrap();
                    if values.len() == 0 {
                        Ok(None)
                    } else {
                        Ok(Some(values.get(0).unwrap().clone()))
                    }
                }
            }
            None => {
                Ok(None)
            }
        },
        Err(error) => {
            Ok(None)
        }
    };
}

// async fn process_instance(instance: RDSInstance) -> Result<(InstanceDetails, f64, f64), Box<dyn Error>> {
// // async fn process_instance(instance: &RDSInstance) -> Result<(), Box<dyn Error>> {
//     // If one of the futures resolves to an error, try_join! will return that error
//     let results = try_join!(
//         get_instance_details(instance),
//         get_instance_metric(instance, "WriteIOPS"),
//         get_instance_metric(instance, "ReadIOPS"),
//     );
//     match results {
//         Ok(r) => {
//             println!("{:?}", r);
//             // println!("{:?}, {:?}, {:?}", d, w, r);
//             IOPS_LIMIT.set(r.0.calculate_limits().unwrap().iops as f64);
//             Ok(())
//         },
//         Err(e) => { Err(e) }
//     }
// }


async fn flatten<T>(handle: JoinHandle<Result<T, String>>) -> Result<T, String> {
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(err) => Err("handling failed".to_string()),
    }
}

async fn process_instance(instance: RDSInstance) {
    let rds_dimensions = vec![
        Dimension { name: "DBInstanceIdentifier".to_string(), value: instance.instance.to_string() },
    ];

    match try_join!(
        flatten(tokio::spawn(get_instance_details(instance.clone()))),
        flatten(tokio::spawn(get_instance_metric(instance.clone(), "AWS/RDS", "WriteIOPS", "Average", rds_dimensions.clone()))),
        flatten(tokio::spawn(get_instance_metric(instance.clone(), "AWS/RDS", "ReadIOPS", "Average", rds_dimensions.clone()))),
        flatten(tokio::spawn(get_instance_metric(instance.clone(), "AWS/RDS", "BurstBalance", "Average", rds_dimensions.clone()))),
    ) {
        Ok(val) => {
            let details = val.0;
            let write_iops = val.1.unwrap_or(0 as f64);
            let read_iops = val.2.unwrap_or(0 as f64);
            let burst_balance_result = val.3;
            let burst_balance = if burst_balance_result.is_some() {
                burst_balance_result.unwrap()
            } else {
                0.0
            };
            let limits = details.calculate_limits().unwrap();

            let mut labels = HashMap::new();
            labels.insert("instance", details.instance_identifier.as_str());

            // throttle iops limit in case burst balance is zero
            if burst_balance_result.is_some() && burst_balance > 0 as f64 {
                IOPS_LIMIT.with(&labels).set(limits.burst_iops as f64);
            } else {
                IOPS_LIMIT.with(&labels).set(limits.iops as f64);
            }

            // burst iops if applicable
            if limits.burst_iops > 0 {
                BURST_IOPS_LIMIT.with(&labels).set(limits.burst_iops as f64);
            }

            // current iops usage
            WRITE_IOPS.with(&labels).set(write_iops as f64);
            READ_IOPS.with(&labels).set(read_iops as f64);
            TOTAL_IOPS.with(&labels).set((write_iops + read_iops) as f64);

            if burst_balance_result.is_some() {
                BURST_CREDIT_BALANCE.with(&labels).set(burst_balance as f64);
            }
        }
        Err(err) => {
            println!("Failed with {}.", err);
        }
    }
}

pub async fn update_values() {
    // todo: read config once
    let config = read_config("config.yaml");
    let mut set = JoinSet::new();
    for instance in config.instances.rds {
        set.spawn(process_instance(instance.clone()));
    }
    while let Some(_res) = set.join_next().await {}
}