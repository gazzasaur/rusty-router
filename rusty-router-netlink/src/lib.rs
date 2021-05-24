use std::sync::Arc;
use std::error::Error;
use async_trait::async_trait;
use std::collections::HashMap;

use log::warn;

use rusty_router_model;
use rusty_router_model::RustyRouter;

pub mod link;
pub mod route;
pub mod packet;
pub mod socket;

pub struct NetlinkRustyRouter {
    config: Arc<rusty_router_model::Router>,
    netlink_socket: Arc<dyn socket::NetlinkSocket + Send + Sync>,
}
impl NetlinkRustyRouter {
    pub fn new(config: rusty_router_model::Router, netlink_socket: Arc<dyn socket::NetlinkSocket + Send + Sync>) -> NetlinkRustyRouter {
        NetlinkRustyRouter {
            config: Arc::new(config), netlink_socket
        }
    }
}
#[async_trait]
impl RustyRouter for NetlinkRustyRouter {
    async fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>, Box<dyn Error>> {
        let config = self.config.clone();
        let netlink_socket = self.netlink_socket.clone();
        let link_module = link::NetlinkRustyRouterLink::new();

        let mut device_id_links = link_module.list_network_interfaces(&netlink_socket).await?;
        let mut device_name_links: HashMap<String, link::NetlinkRustyRouterLinkStatus> = device_id_links.drain().map(|(_, link)| (link.name.clone(), link)).collect();
        let mut device_config: HashMap<&String, String> = config.network_interfaces.iter().map(|(name, config)| (&config.device, name.clone())).collect();

        let mut links = vec![];
        device_name_links.drain().for_each(|(_, link)| links.push(rusty_router_model::NetworkInterfaceStatus::new(
            link.name.clone(), device_config.remove(&link.name), link.state,
        )));
        device_config.drain().for_each(|(device, interface)| links.push(rusty_router_model::NetworkInterfaceStatus::new(
            device.clone(), Some(interface), rusty_router_model::NetworkInterfaceOperationalState::NotFound,
        )));
        Ok(links)
    }

    async fn list_router_interfaces(&self) -> Result<Vec<rusty_router_model::RouterInterfaceStatus>, Box<dyn Error>> {
        let mut device_id_links = link::NetlinkRustyRouterLink::new().list_network_interfaces(&self.netlink_socket).await?;
        let mut device_name_net_name: HashMap<&String, &String> = self.config.network_interfaces.iter().map(|(name, interface)| (&interface.device, name)).collect();
        let mut net_name_addr_name: HashMap<&String, &String> = self.config.router_interfaces.iter().map(|(name, interface)| (&interface.network_interface, name)).collect();

        let mut device_id_addresses = route::NetlinkRustyRouterAddress::new().list_router_interfaces(&self.netlink_socket).await?;
        // let device_config: HashMap<&String, &String> = self.config.network_interfaces.iter().map(|(name, config)| (&config.device, name)).collect();

        let mut addresses = vec![];
        device_id_addresses.drain().for_each(|(index, address_status)| {
            match device_id_links.remove(&index) {
                Some(device) => {
                    let net_name = device_name_net_name.remove(&device.name);
                    let addr_name: Option<String> = net_name.and_then(|net_name| net_name_addr_name.remove(net_name).and_then(|addr_name| Some(addr_name.clone())));
                    addresses.push(rusty_router_model::RouterInterfaceStatus::new(
                        addr_name, address_status.addresses, rusty_router_model::NetworkInterfaceStatus::new(
                            device.name, net_name.map(|value| value.clone()), device.state
                        )
                    ))
                },
                None => warn!("Device {} found in addresses but not devices.", index),
            }
        });
        net_name_addr_name.drain().for_each(|(net_name, addr_name)| {
            let network_interface = match self.config.network_interfaces.get(net_name) {
                Some(network_interface) => rusty_router_model::NetworkInterfaceStatus::new(
                    network_interface.device.clone(), Some(net_name.clone()), rusty_router_model::NetworkInterfaceOperationalState::NotFound
                ),
                None => return,
            };
            addresses.push(rusty_router_model::RouterInterfaceStatus::new(
                Some(addr_name.clone()), vec![], network_interface
            ));
        });
        return Ok(addresses);
    }
}

#[cfg(test)]
mod tests {
    // use super::*;

    #[test]
    fn fetch_no_interfaces() {
        // let config = rusty_router_model::Router {
        //     network_interfaces: HashMap::new(),
        //     vrfs: HashMap::new(),
        // };
        // let mut mock = socket::MockNetlinkSocket::new();
        // mock.expect_send_message().returning(|_| {
        //     Ok(vec![])
        // });
        // assert!(NetlinkRustyRouter::new(config, Box::new(mock)).list_network_interfaces().unwrap().len() == 0);
    }
}
