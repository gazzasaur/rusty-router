use crate::{status::{NetworkInterfaceStatus, NetworkLinkStatus}, NetworkStatusUpdate};
use async_trait::async_trait;
use rusty_router_common::prelude::*;
use tokio::sync::mpsc::Sender;
use std::net::Ipv4Addr;

/**
 * The container for all routing instances.
 *
 * This gives the underlying implementation a higher level management layer to choose what resources
 * should be shared or allocated per routing instance.
 */
#[async_trait]
pub trait RustyRouter {
    async fn fetch_instance(&self) -> Result<Box<dyn RustyRouterInstance + Send + Sync>>;
}

/**
 * A router instance is a collection of resources and APIs to be used by routing protocols.
 */
#[async_trait]
pub trait RustyRouterInstance {
    async fn list_network_links(&self) -> Result<Vec<NetworkLinkStatus>>;
    async fn list_network_interfaces(&self) -> Result<Vec<NetworkInterfaceStatus>>;

    async fn connect_ipv4(
        &self,
        network_interface: &String,
        source: Ipv4Addr,
        protocol: u8,
        multicast_groups: Vec<Ipv4Addr>,
        handler: Box<dyn NetworkEventHandler + Send + Sync>,
    ) -> Result<Box<dyn InetPacketNetworkInterface + Send + Sync>>;

    // TODO Should wrap tokio to avoid framework dependence on the api layer
    async fn subscribe(&self, subscriber: Sender<NetworkStatusUpdate>);
}

#[async_trait]
pub trait NetworkEventHandler {
    async fn on_recv(&self, data: Vec<u8>);
    async fn on_error(&self, message: String);
}

#[async_trait]
pub trait InetPacketNetworkInterface {
    async fn send(&self, to: std::net::Ipv4Addr, data: &Vec<u8>) -> Result<usize>;
}
