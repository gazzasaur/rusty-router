use std::fmt::{Display, Formatter};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkInterfaceStatus {
    pub device: String,
    pub device_binding: NetworkDeviceBinding,
    pub interface_binding: NetworkInterfaceBinding,
    pub operational_state: NetworkInterfaceOperationalState,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum NetworkDeviceBinding {
    Bound,
    Unbound,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum NetworkInterfaceBinding {
    Unbound,
    Bound(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum NetworkInterfaceOperationalState {
    Up,
    Down,
    Unknown,
}

impl Display for NetworkInterfaceOperationalState {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match &self {
            NetworkInterfaceOperationalState::Up => write!(f, "Up"),
            NetworkInterfaceOperationalState::Down => write!(f, "Down"),
            NetworkInterfaceOperationalState::Unknown => write!(f, "Unknown"),
        }
    }
}