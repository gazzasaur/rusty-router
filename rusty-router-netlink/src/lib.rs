use std::error::Error;
use std::collections::HashMap;

use rusty_router_model;
use rusty_router_model::RustyRouter;

pub mod link;
pub mod packet;
pub mod socket;
pub mod address;

pub struct NetlinkRustyRouter {
    config: rusty_router_model::Router,
    netlink_socket: Box<dyn socket::NetlinkSocket>,

    link_module: link::NetlinkRustyRouterLink,
    address_module: address::NetlinkRustyRouterAddress,
}

impl NetlinkRustyRouter {
    pub fn new(config: rusty_router_model::Router, netlink_socket: Box<dyn socket::NetlinkSocket>) -> NetlinkRustyRouter {
        NetlinkRustyRouter {
            config,
            netlink_socket,
            link_module: link::NetlinkRustyRouterLink::new(),
            address_module: address::NetlinkRustyRouterAddress::new()
        }
    }
}

impl RustyRouter for NetlinkRustyRouter {
    fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>, Box<dyn Error>> {
        let mut device_id_links = self.link_module.list_network_interfaces(&self.netlink_socket)?;
        let mut device_name_links: HashMap<String, link::NetlinkRustyRouterLinkStatus> = device_id_links.drain().map(|(_, link)| (link.name.clone(), link)).collect();
        let mut device_config: HashMap<&String, String> = self.config.network_interfaces.iter().map(|(name, config)| (&config.device, name.clone())).collect();

        let mut links = vec![];
        device_name_links.drain().for_each(|(_, link)| links.push(rusty_router_model::NetworkInterfaceStatus {
            interface_binding: device_config.remove(&link.name).map(|name| rusty_router_model::NetworkInterfaceBinding::Bound(name)).unwrap_or(rusty_router_model::NetworkInterfaceBinding::Unbound),
            device_binding: rusty_router_model::NetworkDeviceBinding::Bound,
            device: link.name,
        }));
        device_config.drain().for_each(|(device, interface)| links.push(rusty_router_model::NetworkInterfaceStatus {
            interface_binding: rusty_router_model::NetworkInterfaceBinding::Bound(interface),
            device_binding: rusty_router_model::NetworkDeviceBinding::Unbound,
            device: device.clone(),
        }));
        Ok(links)
    }

    fn list_router_interfaces(&self) -> Result<Vec<rusty_router_model::RouterInterface>, Box<dyn Error>> {
        self.address_module.list_router_interfaces(&self.netlink_socket, &self.link_module.list_network_interfaces(&self.netlink_socket)?).map(|mut interfaces| interfaces.drain().map(|(_, interface)| interface).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_no_interfaces() {
        let config = rusty_router_model::Router {
            network_interfaces: HashMap::new(),
            vrfs: HashMap::new(),
        };
        let mut mock = socket::MockNetlinkSocket::new();
        mock.expect_send_message().returning(|_| {
            Ok(vec![])
        });
        assert!(NetlinkRustyRouter::new(config, Box::new(mock)).list_network_interfaces().unwrap().len() == 0);
    }
}
