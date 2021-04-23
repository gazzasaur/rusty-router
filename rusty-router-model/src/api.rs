use std::error::Error;
use crate::model::NetworkInterface;

pub trait RustyRouter {
    fn list_network_interfaces(&self) -> Result<Vec<NetworkInterface>, Box<dyn Error>>;
}
