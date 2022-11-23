#[derive(Debug)]
pub enum StorageType {
    GP2,
    GP3,
    IO1,
    Unknown
}

impl StorageType {
    pub fn try_from_string(s: &str) -> StorageType {
        match s {
            "gp2" => {StorageType::GP2},
            "gp3" => {StorageType::GP3},
            "io1" => {StorageType::IO1},
            _ => {StorageType::Unknown},
        }
    }
}
