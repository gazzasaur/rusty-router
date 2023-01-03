use std::net::Ipv4Addr;
use std::sync::Arc;
use async_trait::async_trait;
use interface::InterfaceManager;
use rusty_router_common::prelude::*;

use rusty_router_model::{InetPacketNetworkInterface, NetworkEventHandler, NetworkInterfaceStatus, RustyRouter};
use rusty_router_model::RustyRouterInstance;

use crate::network::LinuxInetPacketNetworkInterface;

pub mod network;
pub mod netlink;
pub mod interface;

pub struct LinuxRustyRouter {
    platform: Arc<LinuxRustyRouterPlatform>,
}
impl LinuxRustyRouter {
    pub async fn new(config: rusty_router_model::Router, netlink_socket_factory: Arc<dyn netlink::NetlinkSocketFactory + Send + Sync>) -> Result<LinuxRustyRouter> {
        Ok(LinuxRustyRouter { platform: Arc::new(LinuxRustyRouterPlatform::new(config, netlink_socket_factory).await?) })
    }
}
#[async_trait]
impl RustyRouter for LinuxRustyRouter {
    async fn fetch_instance(&self) -> Result<Box<dyn RustyRouterInstance + Send + Sync>> {
        Ok(Box::new(LinuxRustyRouterInstance::new(self.platform.clone())))
    }
}

pub struct LinuxRustyRouterInstance {
    platform: Arc<LinuxRustyRouterPlatform>
}
impl LinuxRustyRouterInstance {
    fn new(platform: Arc<LinuxRustyRouterPlatform>) -> LinuxRustyRouterInstance {
        LinuxRustyRouterInstance { platform }
    }
}
#[async_trait]
impl RustyRouterInstance for LinuxRustyRouterInstance {
    async fn list_network_links(&self) -> Result<Vec<rusty_router_model::NetworkLinkStatus>> {
        Ok(self.platform.list_network_links().await?)
    }

    async fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>> {
        Ok(self.platform.list_network_interfaces().await?)
    }

    async fn connect_ipv4(&self, network_interface: &String, protocol: i32, multicast_groups: Vec<Ipv4Addr>, handler: Box<dyn NetworkEventHandler + Send + Sync>) -> Result<Box<dyn InetPacketNetworkInterface + Send + Sync>> {
        Ok(self.platform.connect_ipv4(network_interface, protocol, multicast_groups, handler).await?)
    }
}

struct LinuxRustyRouterPlatform {
    network_poller: network::Poller,
    interface_manager: InterfaceManager,
}
impl LinuxRustyRouterPlatform {
    pub async fn new(config: rusty_router_model::Router, netlink_socket_factory: Arc<dyn netlink::NetlinkSocketFactory + Send + Sync>) -> Result<LinuxRustyRouterPlatform> {
        let config = Arc::new(config);
        Ok(LinuxRustyRouterPlatform {
            interface_manager: InterfaceManager::new(config.clone(), netlink_socket_factory).await?,
            network_poller: network::Poller::new()?,
        })
    }
}
#[async_trait]
impl RustyRouterInstance for LinuxRustyRouterPlatform {
    async fn list_network_links(&self) -> Result<Vec<rusty_router_model::NetworkLinkStatus>> {
        Ok(self.interface_manager.list_network_links().await)
    }

    async fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>> {
        Ok(self.interface_manager.list_network_interfaces().await)
    }

    // This pulls all interfaces and filters down.  This is not expected to occur often, but this is expensive.
    // TODO Store the interfaces on the platform.  Subscribe to changes for updates and run anti-entrophy polls.
    async fn connect_ipv4(&self, network_interface: &String, protocol: i32, multicast_groups: Vec<Ipv4Addr>, handler: Box<dyn NetworkEventHandler + Send + Sync>) -> Result<Box<dyn InetPacketNetworkInterface + Send + Sync>> {
        let network_interface: String = network_interface.clone();
        let mut interfaces: Vec<String> = self.list_network_interfaces().await?.drain(..).filter(|interface| {
            // A warning is already emitted if more than one interface with the same name exists
            match interface.get_name() {
                Some(name) => return *name == network_interface,
                None => return false,
            }
        }).map(|network_interface_status: NetworkInterfaceStatus| {
            network_interface_status.get_network_link_status().get_device().clone()
        }).collect();

        match interfaces.pop() {
            Some(device) => return Ok(Box::new(LinuxInetPacketNetworkInterface::new(device, protocol, multicast_groups, handler, &self.network_poller).await?)),
            None => return Err(Error::IllegalState(format!("Failed to find a device matching {}", network_interface))),
        }
    }
}

#[cfg(test)]
mod tests {
}