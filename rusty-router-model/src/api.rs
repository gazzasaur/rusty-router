use std::error::Error;
use crate::status::{NetworkInterfaceStatus, RouterInterfaceStatus};

pub trait RustyRouter {
    fn list_network_interfaces(&self) -> Result<Vec<NetworkInterfaceStatus>, Box<dyn Error>>;
    fn list_router_interfaces(&self) -> Result<Vec<RouterInterfaceStatus>, Box<dyn Error>>;
}
