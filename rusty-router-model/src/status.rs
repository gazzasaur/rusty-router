use std::fmt::{Display, Formatter};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct NetworkLinkStatus {
    name: Option<String>,
    device: String,
    operational_state: NetworkLinkOperationalState,
}
impl NetworkLinkStatus {
    pub fn new(name: Option<String>, device: String, operational_state: NetworkLinkOperationalState) -> NetworkLinkStatus {
        NetworkLinkStatus {
            name, device, operational_state,
        }
    }

    pub fn get_device(&self) -> &String {
        &self.device
    }

    pub fn get_name(&self) -> &Option<String> {
        &self.name
    }

    pub fn get_operational_state(&self) -> &NetworkLinkOperationalState {
        &self.operational_state
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct NetworkInterfaceStatus {
    name: Option<String>,
    addresses: Vec<crate::config::IpAddress>,
    network_link_status: NetworkLinkStatus,
}
impl NetworkInterfaceStatus {
    pub fn new(name: Option<String>, addresses: Vec<crate::config::IpAddress>, network_link_status: NetworkLinkStatus) -> NetworkInterfaceStatus {
        NetworkInterfaceStatus {
            name, addresses, network_link_status,
        }
    }

    pub fn get_name(&self) -> &Option<String> {
        &self.name
    }

    pub fn get_network_link_status(&self) -> &NetworkLinkStatus {
        &self.network_link_status
    }

    pub fn get_addresses(&self) -> &Vec<crate::config::IpAddress> {
        &self.addresses
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum NetworkLinkOperationalState {
    Up,
    Down,
    Unknown,
    NotFound,
}
impl Display for NetworkLinkOperationalState {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match &self {
            NetworkLinkOperationalState::Up => write!(f, "Up"),
            NetworkLinkOperationalState::Down => write!(f, "Down"),
            NetworkLinkOperationalState::Unknown => write!(f, "Unknown"),
            NetworkLinkOperationalState::NotFound => write!(f, "NotFound"),
        }
    }
}