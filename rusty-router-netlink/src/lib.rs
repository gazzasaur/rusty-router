use std::error::Error;

use rusty_router_model;
use rusty_router_model::RustyRouter;

pub mod link;
pub mod packet;
pub mod socket;
pub mod address;

pub struct NetlinkRustyRouter {
    netlink_socket: Box<dyn socket::NetlinkSocket>,

    link_module: link::NetlinkRustyRouterLink,
    address_module: address::NetlinkRustyRouterAddress,
}

impl NetlinkRustyRouter {
    pub fn new(netlink_socket: Box<dyn socket::NetlinkSocket>) -> NetlinkRustyRouter {
        NetlinkRustyRouter {
            netlink_socket,
            link_module: link::NetlinkRustyRouterLink::new(),
            address_module: address::NetlinkRustyRouterAddress::new()
        }
    }
}

impl RustyRouter for NetlinkRustyRouter {
    fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>, Box<dyn Error>> {
        Ok(self.link_module.list_network_interfaces(&self.netlink_socket)?.drain().map(|(_, v)| rusty_router_model::NetworkInterfaceStatus {
            device: v.name,
            device_binding: rusty_router_model::NetworkDeviceBinding::Bound,
            interface_binding: rusty_router_model::NetworkInterfaceBinding::Unbound,
        }).collect())
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
        let mut mock = socket::MockNetlinkSocket::new();
        mock.expect_send_message().returning(|_| {
            Ok(vec![])
        });
        assert!(NetlinkRustyRouter::new(Box::new(mock)).list_network_interfaces().unwrap().len() == 0);
    }
}
