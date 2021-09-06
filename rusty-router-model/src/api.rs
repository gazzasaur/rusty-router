use std::{error::Error, net::Ipv4Addr};
use async_trait::async_trait;
use crate::status::{NetworkLinkStatus, NetworkInterfaceStatus};

#[async_trait]
pub trait NetworkEventHandler {
    async fn on_recv(&self, data: Vec<u8>);
    async fn on_error(&self, message: String);
}

#[async_trait]
pub trait NetworkConnection {
    async fn send(&self, data: Vec<u8>) -> Result<usize, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
pub trait RustyRouter {
    async fn list_network_links(&self) -> Result<Vec<NetworkLinkStatus>, Box<dyn Error>>;
    async fn list_network_interfaces(&self) -> Result<Vec<NetworkInterfaceStatus>, Box<dyn Error>>;

    // TODO Use interface rather than network device
    async fn connect_ipv4(&self, network_device: String, protocol: i32, multicast_groups: Vec<Ipv4Addr>, handler: Box<dyn NetworkEventHandler + Send + Sync>) -> Result<Box<dyn NetworkConnection + Send + Sync>, Box<dyn Error + Send + Sync>>;
}
