use std::error::Error;
use async_trait::async_trait;
use crate::status::{NetworkInterfaceStatus, RouterInterfaceStatus};

#[async_trait]
pub trait NetworkEventHandler {
    async fn on_recv(&self, data: Vec<u8>);
    async fn on_error(&self, message: String);
}

#[async_trait]
pub trait RustyRouter {
    async fn list_network_interfaces(&self) -> Result<Vec<NetworkInterfaceStatus>, Box<dyn Error>>;
    async fn list_router_interfaces(&self) -> Result<Vec<RouterInterfaceStatus>, Box<dyn Error>>;
}
