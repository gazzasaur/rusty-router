use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkInterfaceStatus {
    pub device: String,
    pub device_binding: NetworkDeviceBinding,
    pub interface_binding: NetworkInterfaceBinding,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NetworkDeviceBinding {
    Bound,
    Unbound,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NetworkInterfaceBinding {
    Unbound,
    Bound(String),
}