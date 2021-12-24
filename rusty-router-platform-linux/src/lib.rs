use std::net::Ipv4Addr;
use std::sync::Arc;
use std::error::Error;
use async_trait::async_trait;
use netlink::LogOnlyNetlinkSocketListener;
use std::collections::HashMap;

use log::warn;

use rusty_router_model::{self, InetPacketNetworkInterface, NetworkEventHandler, NetworkInterfaceStatus, RustyRouter};
use rusty_router_model::RustyRouterInstance;

use crate::network::LinuxInetPacketNetworkInterface;

pub mod link;
pub mod route;
pub mod network;
pub mod netlink;

pub struct LinuxRustyRouter {
    platform: Arc<LinuxRustyRouterPlatform>,
}
impl LinuxRustyRouter {
    pub async fn new(config: rusty_router_model::Router, netlink_socket_factory: Arc<dyn netlink::NetlinkSocketFactory + Send + Sync>) -> Result<LinuxRustyRouter, Box<dyn Error + Send + Sync>> {
        Ok(LinuxRustyRouter { platform: Arc::new(LinuxRustyRouterPlatform::new(config, netlink_socket_factory).await?) })
    }
}
#[async_trait]
impl RustyRouter for LinuxRustyRouter {
    async fn fetch_instance(&self) -> Result<Box<dyn RustyRouterInstance + Send + Sync>, Box<dyn Error + Send + Sync>> {
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
    async fn list_network_links(&self) -> Result<Vec<rusty_router_model::NetworkLinkStatus>, Box<dyn Error + Send + Sync>> {
        Ok(self.platform.list_network_links().await?)
    }

    async fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>, Box<dyn Error + Send + Sync>> {
        Ok(self.platform.list_network_interfaces().await?)
    }

    async fn connect_ipv4(&self, network_interface: &String, protocol: i32, multicast_groups: Vec<Ipv4Addr>, handler: Box<dyn NetworkEventHandler + Send + Sync>) -> Result<Box<dyn InetPacketNetworkInterface + Send + Sync>, Box<dyn Error + Send + Sync>> {
        Ok(self.platform.connect_ipv4(network_interface, protocol, multicast_groups, handler).await?)
    }
}

struct LinuxRustyRouterPlatform {
    network_poller: network::Poller,
    config: Arc<rusty_router_model::Router>,
    netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync>,
}
impl LinuxRustyRouterPlatform {
    pub async fn new(config: rusty_router_model::Router, netlink_socket_factory: Arc<dyn netlink::NetlinkSocketFactory + Send + Sync>) -> Result<LinuxRustyRouterPlatform, Box<dyn Error + Send + Sync>> {
        Ok(LinuxRustyRouterPlatform {
            netlink_socket: netlink_socket_factory.create_socket(Arc::new(LogOnlyNetlinkSocketListener::new())).await?,
            config: Arc::new(config),
            network_poller: network::Poller::new()?,
        })
    }

    async fn perform_list_network_interfaces(&self) -> Result<HashMap<u64, link::NetlinkRustyRouterLinkStatus>, Box<dyn Error + Send + Sync>> {
        #[cfg(test)] return crate::tests::list_network_interfaces(&self.netlink_socket).await;
        #[cfg(not(test))] return link::NetlinkRustyRouterLink::list_network_interfaces(&self.netlink_socket).await;
    }

    async fn perform_list_router_interfaces(&self) -> Result<HashMap<u64, route::NetlinkRustyRouterDeviceAddressesResult>, Box<dyn Error + Send + Sync>> {
        #[cfg(test)] return crate::tests::list_router_interfaces(&self.netlink_socket).await;
        #[cfg(not(test))] return route::NetlinkRustyRouterAddress::list_router_interfaces(&self.netlink_socket).await;
    }

    async fn perform_list_mapped_router_interfaces(&self, network_router_interfaces: &mut HashMap<&String, &String>, addresses: &mut Vec<rusty_router_model::NetworkInterfaceStatus>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut device_id_links = self.perform_list_network_interfaces().await?;
        let mut device_id_addresses = self.perform_list_router_interfaces().await?;
        let mut device_network_interfaces: HashMap<&String, &String> = self.config.get_network_interfaces().iter().map(|(name, interface)| (interface.get_device(), name)).collect();
    
        device_id_addresses.drain().for_each(|(index, mut address_status)| {
            match device_id_links.remove(&index) {
                Some(device) => {
                    let net_name = device_network_interfaces.remove(&device.get_name().clone());
                    let addr_name: Option<String> = net_name.and_then(|net_name| network_router_interfaces.remove(net_name).and_then(|addr_name| Some(addr_name.clone())));
                    addresses.push(rusty_router_model::NetworkInterfaceStatus::new(
                        addr_name, address_status.take_addresses(), rusty_router_model::NetworkLinkStatus::new(
                            device.get_name().clone(), net_name.map(|value| value.clone()), device.get_state().clone()
                        )
                    ))
                },
                None => warn!("Ignoring device {}.  Found in addresses but not devices.", index),
            }
        });
        Ok(())
    }
}
#[async_trait]
impl RustyRouterInstance for LinuxRustyRouterPlatform {
    async fn list_network_links(&self) -> Result<Vec<rusty_router_model::NetworkLinkStatus>, Box<dyn Error + Send + Sync>> {
        let config = self.config.clone();
        let mut device_id_links = self.perform_list_network_interfaces().await?;
        let mut device_name_links: HashMap<String, link::NetlinkRustyRouterLinkStatus> = device_id_links.drain().map(|(_, link)| (link.get_name().clone(), link)).collect();
        let mut device_config: HashMap<&String, String> = config.get_network_interfaces().iter().map(|(name, config)| (config.get_device(), name.clone())).collect();

        let mut links = vec![];
        device_name_links.drain().for_each(|(_, link)| links.push(rusty_router_model::NetworkLinkStatus::new(
            link.get_name().clone(), device_config.remove(&link.get_name().clone()), link.get_state().clone(),
        )));
        device_config.drain().for_each(|(device, interface)| links.push(rusty_router_model::NetworkLinkStatus::new(
            device.clone(), Some(interface), rusty_router_model::NetworkLinkOperationalState::NotFound,
        )));
        Ok(links)
    }

    async fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>, Box<dyn Error + Send + Sync>> {
        let mut addresses = vec![];
        let mut network_router_interfaces: HashMap<&String, &String> = self.config.get_router_interfaces().iter().map(|(name, interface)| (interface.get_network_interface(), name)).collect();

        self.perform_list_mapped_router_interfaces(&mut network_router_interfaces, &mut addresses).await?;
        network_router_interfaces.drain().for_each(|(net_name, addr_name)| {
            let network_interface = match self.config.get_network_interfaces().get(net_name) {
                Some(network_interface) => rusty_router_model::NetworkLinkStatus::new(
                    network_interface.get_device().clone(), Some(net_name.clone()), rusty_router_model::NetworkLinkOperationalState::NotFound
                ),
                None => return
            };
            addresses.push(rusty_router_model::NetworkInterfaceStatus::new(
                Some(addr_name.clone()), vec![], network_interface
            ));
        });
        return Ok(addresses);
    }

    // This pulls all interfaces and filters down.  This is not expected to occur often, but this is expensive.
    // TODO Store the interfaces on the platform.  Subscribe to changes for updates and run anti-entrophy polls.
    async fn connect_ipv4(&self, network_interface: &String, protocol: i32, multicast_groups: Vec<Ipv4Addr>, handler: Box<dyn NetworkEventHandler + Send + Sync>) -> Result<Box<dyn InetPacketNetworkInterface + Send + Sync>, Box<dyn Error + Send + Sync>> {
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
            None => return Err(Box::from(anyhow::anyhow!("Failed to find a device matching {}", network_interface))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref LINK_TEST_DATA: Arc<Mutex<HashMap<usize, HashMap<u64, link::NetlinkRustyRouterLinkStatus>>>> = Arc::new(Mutex::new(HashMap::new()));
        static ref ROUTE_TEST_DATA: Arc<Mutex<HashMap<usize, HashMap<u64, route::NetlinkRustyRouterDeviceAddressesResult>>>> = Arc::new(Mutex::new(HashMap::new()));
    }

    pub async fn list_network_interfaces(socket: &Arc<dyn netlink::NetlinkSocket + Send + Sync>) -> Result<HashMap<u64, link::NetlinkRustyRouterLinkStatus>, Box<dyn Error + Send + Sync>> {
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() { if let Some(test_value) = test_data.remove(&(Arc::as_ptr(socket) as *const () as usize)) {
            return Ok(test_value)
        }}
        panic!("No test data found");
    }

    pub async fn list_router_interfaces(socket: &Arc<dyn netlink::NetlinkSocket + Send + Sync>) -> Result<HashMap<u64, route::NetlinkRustyRouterDeviceAddressesResult>, Box<dyn Error + Send + Sync>> {
        if let Ok(mut test_data) = ROUTE_TEST_DATA.lock() { if let Some(test_value) = test_data.remove(&(Arc::as_ptr(socket) as *const () as usize)) {
            return Ok(test_value)
        }}
        panic!("No test data found");
    }

    #[tokio::test]
    async fn test_list_network_interfaces_empty() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync> = Arc::new(netlink::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }

        let mut mock_netlink_socket_factory = netlink::MockNetlinkSocketFactory::new();
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| Ok(mock_netlink_socket.clone()));

        let subject = LinuxRustyRouterPlatform::new(config, Arc::new(mock_netlink_socket_factory)).await?;
        match subject.list_network_links().await {
            Ok(actual) => assert_eq!(actual, vec![]),
            Err(_) => panic!("Test Failed"),
        };
        Ok(())
    }
    
    #[tokio::test]
    async fn test_list_network_interfaces_unassigned_up() -> Result<(), Box<dyn Error + Send + Sync>> {
        perform_list_network_interfaces_unassigned(rusty_router_model::NetworkLinkOperationalState::Up).await
    }

    #[tokio::test]
    async fn test_list_network_interfaces_unassigned_down() -> Result<(), Box<dyn Error + Send + Sync>> {
        perform_list_network_interfaces_unassigned(rusty_router_model::NetworkLinkOperationalState::Down).await
    }

    async fn perform_list_network_interfaces_unassigned(state: rusty_router_model::NetworkLinkOperationalState) -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync> = Arc::new(netlink::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, vec![(
                100 as u64,
                link::NetlinkRustyRouterLinkStatus::new(100, "test_iface".to_string(), state.clone())
            )].into_iter().collect());
        }

        let mut mock_netlink_socket_factory = netlink::MockNetlinkSocketFactory::new();
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| Ok(mock_netlink_socket.clone()));

        let subject = LinuxRustyRouterPlatform::new(config, Arc::new(mock_netlink_socket_factory)).await?;
        match subject.list_network_links().await {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkLinkStatus::new("test_iface".to_string(), None, state.clone() )]),
            Err(_) => panic!("Test Failed"),
        };
        Ok(())
    }

    #[tokio::test]
    async fn test_list_network_interfaces_missing() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = rusty_router_model::Router::new(vec![
            ("iface0".to_string(), rusty_router_model::NetworkInterface::new("eth0".to_string(), rusty_router_model::NetworkInterfaceType::GenericInterface))
        ].into_iter().collect(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync> = Arc::new(netlink::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }

        let mut mock_netlink_socket_factory = netlink::MockNetlinkSocketFactory::new();
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| Ok(mock_netlink_socket.clone()));

        let subject = LinuxRustyRouterPlatform::new(config, Arc::new(mock_netlink_socket_factory)).await?;
        match subject.list_network_links().await {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkLinkStatus::new("eth0".to_string(), Some("iface0".to_string()), rusty_router_model::NetworkLinkOperationalState::NotFound)]),
            Err(_) => panic!("Test Failed"),
        };
        Ok(())
    }

    #[tokio::test]
    async fn test_list_network_interfaces_existing() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = rusty_router_model::Router::new(vec![
            ("iface0".to_string(), rusty_router_model::NetworkInterface::new("eth0".to_string(), rusty_router_model::NetworkInterfaceType::GenericInterface))
        ].into_iter().collect(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync> = Arc::new(netlink::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, vec![(
                100 as u64,
                link::NetlinkRustyRouterLinkStatus::new(100, "eth0".to_string(), rusty_router_model::NetworkLinkOperationalState::Up)
            )].into_iter().collect());
        }

        let mut mock_netlink_socket_factory = netlink::MockNetlinkSocketFactory::new();
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| Ok(mock_netlink_socket.clone()));

        let subject = LinuxRustyRouterPlatform::new(config, Arc::new(mock_netlink_socket_factory)).await?;
        match subject.list_network_links().await {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkLinkStatus::new("eth0".to_string(), Some("iface0".to_string()), rusty_router_model::NetworkLinkOperationalState::Up)]),
            Err(_) => panic!("Test Failed"),
        };
        Ok(())
    }

    #[tokio::test]
    async fn test_list_router_interfaces_empty() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync> = Arc::new(netlink::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }
        if let Ok(mut test_data) = ROUTE_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }

        let mut mock_netlink_socket_factory = netlink::MockNetlinkSocketFactory::new();
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| Ok(mock_netlink_socket.clone()));

        let subject = LinuxRustyRouterPlatform::new(config, Arc::new(mock_netlink_socket_factory)).await?;
        match subject.list_network_interfaces().await {
            Ok(actual) => assert_eq!(actual, vec![]),
            Err(_) => panic!("Test Failed"),
        };
        Ok(())
    }

    #[tokio::test]
    async fn test_list_router_interfaces_mapped() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync> = Arc::new(netlink::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, vec![(
                100 as u64,
                link::NetlinkRustyRouterLinkStatus::new(100, "eth0".to_string(), rusty_router_model::NetworkLinkOperationalState::Up)
            )].into_iter().collect());
        }
        if let Ok(mut test_data) = ROUTE_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, vec![(
                100 as u64,
                route::NetlinkRustyRouterDeviceAddressesResult::new(vec![rusty_router_model::IpAddress::new(rusty_router_model::IpAddressType::IpV4, "127.0.0.1".to_string(), 32)])
            )].into_iter().collect());
        }

        let mut mock_netlink_socket_factory = netlink::MockNetlinkSocketFactory::new();
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| Ok(mock_netlink_socket.clone()));

        let subject = LinuxRustyRouterPlatform::new(config, Arc::new(mock_netlink_socket_factory)).await?;
        match subject.list_network_interfaces().await {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkInterfaceStatus::new(
                None, vec![rusty_router_model::IpAddress::new(
                    rusty_router_model::IpAddressType::IpV4, "127.0.0.1".to_string(), 32
                )], rusty_router_model::NetworkLinkStatus::new(
                    "eth0".to_string(), None, rusty_router_model::NetworkLinkOperationalState::Up
                )
            )]),
            Err(_) => panic!("Test Failed"),
        };
        Ok(())
    }

    #[tokio::test]
    async fn test_list_router_interfaces_unmapped() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = rusty_router_model::Router::new(vec![
            ("iface0".to_string(), rusty_router_model::NetworkInterface::new("eth0".to_string(), rusty_router_model::NetworkInterfaceType::GenericInterface)),
        ].into_iter().collect(), vec![
            ("Link1".to_string(), rusty_router_model::RouterInterface::new(None, "iface0".to_string(), vec![])),
            ("Link2".to_string(), rusty_router_model::RouterInterface::new(None, "iface1".to_string(), vec![])),
        ].into_iter().collect(), HashMap::new());

        let mock_netlink_socket: Arc<dyn netlink::NetlinkSocket + Send + Sync> = Arc::new(netlink::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }
        if let Ok(mut test_data) = ROUTE_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }

        let mut mock_netlink_socket_factory = netlink::MockNetlinkSocketFactory::new();
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| Ok(mock_netlink_socket.clone()));

        let subject = LinuxRustyRouterPlatform::new(config, Arc::new(mock_netlink_socket_factory)).await?;
        match subject.list_network_interfaces().await {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkInterfaceStatus::new(
                Some("Link1".to_string()), vec![], rusty_router_model::NetworkLinkStatus::new(
                    "eth0".to_string(), Some("iface0".to_string()), rusty_router_model::NetworkLinkOperationalState::NotFound
                )
            )]),
            Err(_) => panic!("Test Failed"),
        };
        Ok(())
    }
}