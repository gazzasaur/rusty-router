use std::error::Error;
use crate::config::RouterInterface;
use crate::status::NetworkInterfaceStatus;

pub trait RustyRouter {
    fn list_network_interfaces(&self) -> Result<Vec<NetworkInterfaceStatus>, Box<dyn Error>>;
    fn list_router_interfaces(&self) -> Result<Vec<RouterInterface>, Box<dyn Error>>;
}
