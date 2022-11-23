use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct RDSInstance {
    pub region: String,
    pub instance: String,
    pub aws_access_key: String,
    pub aws_secret_key: String,
}


#[derive(Deserialize, Debug)]
pub struct InstanceList {
    pub rds: Vec<RDSInstance>,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub enabled_metrics: Vec<String>,
    pub instances: InstanceList,
}

pub fn read_config(file_path: &str) -> Config {
    let f = std::fs::File::open(file_path).unwrap();
    serde_yaml::from_reader(f).unwrap()
}