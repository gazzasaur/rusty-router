use std::error::Error;
use async_trait::async_trait;
use crate::status::{NetworkInterfaceStatus, RouterInterfaceStatus};

#[async_trait]
pub trait RustyRouter {
    async fn list_network_interfaces(&self) -> Result<Vec<NetworkInterfaceStatus>, Box<dyn Error>>;
    async fn list_router_interfaces(&self) -> Result<Vec<RouterInterfaceStatus>, Box<dyn Error>>;
}
