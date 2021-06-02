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
            config: Arc::new(config),
            netlink_socket: netlink_socket,
        }
    }

    async fn perform_list_network_interfaces(&self) -> Result<HashMap<u64, link::NetlinkRustyRouterLinkStatus>, Box<dyn Error>> {
        #[cfg(test)] return crate::tests::list_network_interfaces(&self.netlink_socket).await;
        #[cfg(not(test))] return link::NetlinkRustyRouterLink::list_network_interfaces(&self.netlink_socket).await;
    }

    async fn perform_list_router_interfaces(&self) -> Result<HashMap<u64, route::NetlinkRustyRouterDeviceAddressesResult>, Box<dyn Error>> {
        #[cfg(test)] return crate::tests::list_router_interfaces(&self.netlink_socket).await;
        #[cfg(not(test))] return route::NetlinkRustyRouterAddress::list_router_interfaces(&self.netlink_socket).await;
    }
}
#[async_trait]
impl RustyRouter for NetlinkRustyRouter {
    async fn list_network_interfaces(&self) -> Result<Vec<rusty_router_model::NetworkInterfaceStatus>, Box<dyn Error>> {
        let config = self.config.clone();
        let mut device_id_links = self.perform_list_network_interfaces().await?;
        let mut device_name_links: HashMap<String, link::NetlinkRustyRouterLinkStatus> = device_id_links.drain().map(|(_, link)| (link.get_name().clone(), link)).collect();
        let mut device_config: HashMap<&String, String> = config.get_network_interfaces().iter().map(|(name, config)| (&config.device, name.clone())).collect();

        let mut links = vec![];
        device_name_links.drain().for_each(|(_, link)| links.push(rusty_router_model::NetworkInterfaceStatus::new(
            link.get_name().clone(), device_config.remove(&link.get_name().clone()), link.get_state().clone(),
        )));
        device_config.drain().for_each(|(device, interface)| links.push(rusty_router_model::NetworkInterfaceStatus::new(
            device.clone(), Some(interface), rusty_router_model::NetworkInterfaceOperationalState::NotFound,
        )));
        Ok(links)
    }

    async fn list_router_interfaces(&self) -> Result<Vec<rusty_router_model::RouterInterfaceStatus>, Box<dyn Error>> {
        let mut device_id_links = self.perform_list_network_interfaces().await?;
        let mut device_name_net_name: HashMap<&String, &String> = self.config.get_network_interfaces().iter().map(|(name, interface)| (&interface.device, name)).collect();
        let mut net_name_addr_name: HashMap<&String, &String> = self.config.get_router_interfaces().iter().map(|(name, interface)| (&interface.network_interface, name)).collect();
        let mut device_id_addresses = self.perform_list_router_interfaces().await?;

        let mut addresses = vec![];
        device_id_addresses.drain().for_each(|(index, mut address_status)| {
            match device_id_links.remove(&index) {
                Some(device) => {
                    let net_name = device_name_net_name.remove(&device.get_name().clone());
                    let addr_name: Option<String> = net_name.and_then(|net_name| net_name_addr_name.remove(net_name).and_then(|addr_name| Some(addr_name.clone())));
                    addresses.push(rusty_router_model::RouterInterfaceStatus::new(
                        addr_name, address_status.take_addresses(), rusty_router_model::NetworkInterfaceStatus::new(
                            device.get_name().clone(), net_name.map(|value| value.clone()), device.get_state().clone()
                        )
                    ))
                },
                None => warn!("Ignoring device {}.  Found in addresses but not devices.", index),
            }
        });
        net_name_addr_name.drain().for_each(|(net_name, addr_name)| {
            let network_interface = match self.config.get_network_interfaces().get(net_name) {
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
    use super::*;
    use std::sync::Mutex;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref LINK_TEST_DATA: Arc<Mutex<HashMap<usize, HashMap<u64, link::NetlinkRustyRouterLinkStatus>>>> = Arc::new(Mutex::new(HashMap::new()));
        static ref ROUTE_TEST_DATA: Arc<Mutex<HashMap<usize, HashMap<u64, route::NetlinkRustyRouterDeviceAddressesResult>>>> = Arc::new(Mutex::new(HashMap::new()));
    }

    pub async fn list_network_interfaces(socket: &Arc<dyn socket::NetlinkSocket + Send + Sync>) -> Result<HashMap<u64, link::NetlinkRustyRouterLinkStatus>, Box<dyn Error>> {
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() { if let Some(test_value) = test_data.remove(&(Arc::as_ptr(socket) as *const () as usize)) {
            return Ok(test_value)
        }}
        panic!("No test data found");
    }

    pub async fn list_router_interfaces(socket: &Arc<dyn socket::NetlinkSocket + Send + Sync>) -> Result<HashMap<u64, route::NetlinkRustyRouterDeviceAddressesResult>, Box<dyn Error>> {
        if let Ok(mut test_data) = ROUTE_TEST_DATA.lock() { if let Some(test_value) = test_data.remove(&(Arc::as_ptr(socket) as *const () as usize)) {
            return Ok(test_value)
        }}
        panic!("No test data found");
    }

    #[test]
    fn test_list_network_interfaces_empty() {
        let config = rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn socket::NetlinkSocket + Send + Sync> = Arc::new(crate::socket::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }

        let subject = NetlinkRustyRouter { config: Arc::new(config), netlink_socket: mock_netlink_socket.clone() };
        match tokio_test::block_on(subject.list_network_interfaces()) {
            Ok(actual) => assert_eq!(actual, vec![]),
            Err(_) => panic!("Test Failed"),
        };
    }
    
    #[test]
    fn test_list_network_interfaces_unassigned_up() {
        perform_list_network_interfaces_unassigned(rusty_router_model::NetworkInterfaceOperationalState::Up)
    }

    #[test]
    fn test_list_network_interfaces_unassigned_down() {
        perform_list_network_interfaces_unassigned(rusty_router_model::NetworkInterfaceOperationalState::Down)
    }

    fn perform_list_network_interfaces_unassigned(state: rusty_router_model::NetworkInterfaceOperationalState) {
        let config = rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn socket::NetlinkSocket + Send + Sync> = Arc::new(crate::socket::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, vec![(
                100 as u64,
                link::NetlinkRustyRouterLinkStatus::new(100, "test_iface".to_string(), state.clone())
            )].into_iter().collect());
        }

        let subject = NetlinkRustyRouter { config: Arc::new(config), netlink_socket: mock_netlink_socket.clone() };
        match tokio_test::block_on(subject.list_network_interfaces()) {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkInterfaceStatus::new("test_iface".to_string(), None, state.clone() )]),
            Err(_) => panic!("Test Failed"),
        };
    }

    #[test]
    fn test_list_network_interfaces_missing() {
        let config = rusty_router_model::Router::new(vec![
            ("iface0".to_string(), rusty_router_model::NetworkInterface {
                device: "eth0".to_string(),
                network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
            })
        ].into_iter().collect(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn socket::NetlinkSocket + Send + Sync> = Arc::new(crate::socket::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }

        let subject = NetlinkRustyRouter { config: Arc::new(config), netlink_socket: mock_netlink_socket.clone() };
        match tokio_test::block_on(subject.list_network_interfaces()) {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkInterfaceStatus::new("eth0".to_string(), Some("iface0".to_string()), rusty_router_model::NetworkInterfaceOperationalState::NotFound)]),
            Err(_) => panic!("Test Failed"),
        };
    }

    #[test]
    fn test_list_network_interfaces_existing() {
        let config = rusty_router_model::Router::new(vec![
            ("iface0".to_string(), rusty_router_model::NetworkInterface {
                device: "eth0".to_string(),
                network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
            })
        ].into_iter().collect(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn socket::NetlinkSocket + Send + Sync> = Arc::new(crate::socket::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, vec![(
                100 as u64,
                link::NetlinkRustyRouterLinkStatus::new(100, "eth0".to_string(), rusty_router_model::NetworkInterfaceOperationalState::Up)
            )].into_iter().collect());
        }

        let subject = NetlinkRustyRouter { config: Arc::new(config), netlink_socket: mock_netlink_socket.clone() };
        match tokio_test::block_on(subject.list_network_interfaces()) {
            Ok(actual) => assert_eq!(actual, vec![rusty_router_model::NetworkInterfaceStatus::new("eth0".to_string(), Some("iface0".to_string()), rusty_router_model::NetworkInterfaceOperationalState::Up)]),
            Err(_) => panic!("Test Failed"),
        };
    }

    #[test]
    fn test_list_router_interfaces_empty() {
        let config = rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new());

        let mock_netlink_socket: Arc<dyn socket::NetlinkSocket + Send + Sync> = Arc::new(crate::socket::MockNetlinkSocket::new());
        if let Ok(mut test_data) = LINK_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }
        if let Ok(mut test_data) = ROUTE_TEST_DATA.lock() {
            test_data.insert(Arc::as_ptr(&mock_netlink_socket) as * const () as usize, HashMap::new());
        }

        let subject = NetlinkRustyRouter { config: Arc::new(config), netlink_socket: mock_netlink_socket.clone() };
        match tokio_test::block_on(subject.list_router_interfaces()) {
            Ok(actual) => assert_eq!(actual, vec![]),
            Err(_) => panic!("Test Failed"),
        };
    }
}