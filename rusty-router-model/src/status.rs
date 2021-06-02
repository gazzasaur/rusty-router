use std::fmt::{Display, Formatter};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct NetworkInterfaceStatus {
    device: String,
    name: Option<String>,
    operational_state: NetworkInterfaceOperationalState,
}
impl NetworkInterfaceStatus {
    pub fn new(device: String, name: Option<String>, operational_state: NetworkInterfaceOperationalState) -> NetworkInterfaceStatus {
        NetworkInterfaceStatus {
            name, device, operational_state,
        }
    }

    pub fn get_device(&self) -> &String {
        &self.device
    }

    pub fn get_name(&self) -> &Option<String> {
        &self.name
    }

    pub fn get_operational_state(&self) -> &NetworkInterfaceOperationalState {
        &self.operational_state
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct RouterInterfaceStatus {
    name: Option<String>,
    addresses: Vec<crate::config::IpAddress>,
    network_interface_status: NetworkInterfaceStatus,
}
impl RouterInterfaceStatus {
    pub fn new(name: Option<String>, addresses: Vec<crate::config::IpAddress>, network_interface_status: NetworkInterfaceStatus) -> RouterInterfaceStatus {
        RouterInterfaceStatus {
            name, addresses, network_interface_status,
        }
    }

    pub fn get_name(&self) -> &Option<String> {
        &self.name
    }

    pub fn get_network_interface_status(&self) -> &NetworkInterfaceStatus {
        &self.network_interface_status
    }

    pub fn get_addresses(&self) -> &Vec<crate::config::IpAddress> {
        &self.addresses
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum NetworkInterfaceOperationalState {
    Up,
    Down,
    Unknown,
    NotFound,
}
impl Display for NetworkInterfaceOperationalState {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match &self {
            NetworkInterfaceOperationalState::Up => write!(f, "Up"),
            NetworkInterfaceOperationalState::Down => write!(f, "Down"),
            NetworkInterfaceOperationalState::Unknown => write!(f, "Unknown"),
            NetworkInterfaceOperationalState::NotFound => write!(f, "NotFound"),
        }
    }
}