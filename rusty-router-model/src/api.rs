use std::{error::Error, net::Ipv4Addr};
use async_trait::async_trait;
use crate::status::{NetworkLinkStatus, NetworkInterfaceStatus};

#[async_trait]
pub trait NetworkEventHandler {
    async fn on_recv(&self, data: Vec<u8>);
    async fn on_error(&self, message: String);
}

#[async_trait]
pub trait InetPacketNetworkInterface {
    async fn send(&self, to: std::net::Ipv4Addr, data: Vec<u8>) -> Result<usize, Box<dyn std::error::Error + Send + Sync>>;
}

#[async_trait]
pub trait RustyRouter {
    async fn list_network_links(&self) -> Result<Vec<NetworkLinkStatus>, Box<dyn Error + Send + Sync>>;
    async fn list_network_interfaces(&self) -> Result<Vec<NetworkInterfaceStatus>, Box<dyn Error + Send + Sync>>;

    async fn connect_ipv4(&self, network_interface: &String, protocol: i32, multicast_groups: Vec<Ipv4Addr>, handler: Box<dyn NetworkEventHandler + Send + Sync>) -> Result<Box<dyn InetPacketNetworkInterface + Send + Sync>, Box<dyn Error + Send + Sync>>;
}
