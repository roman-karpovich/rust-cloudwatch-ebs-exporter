mod rds_metric;
mod config;
mod constants;

use clap::{Arg, Command};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use prometheus::{Encoder, TextEncoder};
use std::convert::Infallible;
use std::error::Error;
use std::sync::{Arc, Mutex};
use futures::{try_join};
use futures::executor::block_on;
use futures::future::join_all;
use hyper::server::conn::AddrStream;
// use tokio::task::JoinHandle;
use crate::config::{RDSInstance, read_config};
use crate::rds_metric::{get_instance_details, get_instance_metric, InstanceDetails, IOPS_LIMIT};

async fn serve_req(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let encoder = TextEncoder::new();

    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    // tokio::runtime::Runtime::new().unwrap().block_on(rds_metric::update_values());
    rds_metric::update_values().await;

    Ok(Response::new(Body::from(buffer)))
}

async fn server_req1(_req: Request<Body>) -> Result<Response<Body>, hyper::Error>{
    let encoder = TextEncoder::new();

    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    rds_metric::update_values().await;

    Ok(Response::new(Body::from(buffer)))
}

#[tokio::main]
async fn main() {
    // println!("{:?}", results);
    // for instance in config.instances.rds {
    //     process_instance(&instance).await;
    // }
    //     let local_futures = vec![
    //
    //     ];
    //     // all_futures.extend(local_futures);
    //
    //     // If one of the futures resolves to an error, try_join! will return that error
    //     // let results = try_join!(
    //     //     get_instance_details(&instance),
    //     //     get_instance_metric(&instance, "WriteIOPS"),
    //     //     get_instance_metric(&instance, "ReadIOPS"),
    //     // );
    //     // println!("{:?}", results);
    //
    //     // futures.append(flatten(tokio::spawn()));
    //     // futures.append(flatten(tokio::spawn()));
    //     // futures.append(flatten(tokio::spawn()));
    // }

    // let results = try_join!(all_futures);
    // println!("{:?}", results);
    // match tokio::try_join!(futures) {
    //     Ok(values) => {
    //         println!("{:?}", values);
    //         // do something with the values
    //     }
    //     Err(err) => {
    //         println!("Failed with {}.", err);
    //     }
    // }
    // return Ok(());
    let matches = Command::new("Prometheus request time tracker exporter")
        .version("0.0.1")
        .author("Roman Karpovich <fpm.th13f@gmail.com>")
        .about("export ebs important metrics")
        .arg(Arg::new("port").help("port to listen").long("port").takes_value(true))
        .get_matches();

    let port = matches.value_of("port").unwrap_or("9187").parse::<u16>().unwrap();

    let addr = ([0, 0, 0, 0], port).into();
    println!("Listening on http://{}", addr);

    // let make_service = make_service_fn(move |_conn| {
    //     async move {
    //         Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
    //             async move { Ok::<_, Infallible>(serve_req(req)) }
    //         }))
    //     }
    // });

    // let make_svc = make_service_fn(|socket: &AddrStream| {
    //     let remote_addr = socket.remote_addr();
    //     async move {
    //         Ok::<_, Infallible>(service_fn(move |_: Request<Body>| async move {
    //             Ok::<_, Infallible>(
    //                 Response::new(Body::from(format!("Hello, {}!", remote_addr)))
    //             )
    //         }))
    //     }
    // });

    let make_svc = make_service_fn(|socket: &AddrStream| {
    let remote_addr = socket.remote_addr();
    async move {
        // Ok::<_, Infallible>(service_fn(|req: Request<Body>| serve_req(req)))
        Ok::<_, Infallible>(service_fn(|req: Request<Body>| server_req1(req)))
    }
});

    let serve_future = Server::bind(&addr).serve(make_svc);

    if let Err(err) = serve_future.await {
        eprintln!("server error: {}", err);
    }
}
