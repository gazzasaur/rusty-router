use std::error::Error;
use crate::model::NetworkInterface;
use crate::model::RouterInterface;

pub trait RustyRouter {
    fn list_network_interfaces(&self) -> Result<Vec<NetworkInterface>, Box<dyn Error>>;
    fn list_router_interfaces(&self) -> Result<Vec<RouterInterface>, Box<dyn Error>>;
}
