use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::ops::Add;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{sync::Arc, error::Error};

use log::{error, warn};
use async_trait::async_trait;
use netlink_packet_route::link::nlas;
use rusty_router_model::{NetworkInterfaceStatus, NetworkLinkStatus};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::netlink::NetlinkSocket;
use crate::netlink::NetlinkSocketFactory;
use crate::netlink::NetlinkSocketListener;
use crate::netlink::build_default_packet;

const ENTROPY_HOLD_PERIOD_SECONDS: u64 = 5;
const ENTROPY_SCAN_PERIOD_SECONDS: u64 = 10;

/**
 * Two mechanisms exist to maintain a list of network interface information.
 * A listener will subscribe to all networking events to ensure we have the latest information, but this can be lossy.
 * An anti-entrophy scan is carried out periodically to ensure missing or incomplete information is captured.
 */
pub struct InterfaceManager {
    running: Arc<AtomicBool>,
    data: Arc<RwLock<InterfaceManagerData>>,
}
impl InterfaceManager {
    pub async fn new(config: Arc<rusty_router_model::Router>, netlink_socket_factory: Arc<dyn NetlinkSocketFactory + Send + Sync>) -> Result<InterfaceManager, Box<dyn Error + Send + Sync>> {
        let netlink_message_processor = Arc::new(NetlinkMessageProcessor::new(config));
        let data = Arc::new(RwLock::new(InterfaceManagerData::new()));
        let netlink_socket = netlink_socket_factory.create_socket(Box::new(InterfaceManagerNetlinkSocketListener::new(netlink_message_processor.clone(), data.clone()))).await?;
        let running = Arc::new(AtomicBool::new(true));
        InterfaceManagerWorker::start(netlink_message_processor, running.clone(), netlink_socket.clone(), data.clone()).await;
        Ok(InterfaceManager { running, data })
    }

    pub async fn list_network_links(&self) -> Vec<NetworkLinkStatus> {
        self.data.read().await.list_link_status()
    }

    pub async fn list_network_interfaces(&self) -> Vec<NetworkInterfaceStatus> {
        self.data.read().await.list_interface_status()
    }
}
impl Drop for InterfaceManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

struct InterfaceManagerData {
    mapped_network_links: HashMap<String, Arc<NetworkStatusItem<NetworkLinkStatus>>>,
    network_links: HashMap<CanonicalNetworkId, Arc<NetworkStatusItem<NetworkLinkStatus>>>,
    mapped_network_interfaces: HashMap<String, Arc<NetworkStatusItem<NetworkInterfaceStatus>>>,
    network_interfaces: HashMap<CanonicalNetworkId, Arc<NetworkStatusItem<NetworkInterfaceStatus>>>,
}
impl InterfaceManagerData {
    pub fn new() -> InterfaceManagerData {
        InterfaceManagerData { network_links: HashMap::new(), mapped_network_links: HashMap::new(), network_interfaces: HashMap::new(), mapped_network_interfaces: HashMap::new() }
    }

    pub fn list_link_status(&self) -> Vec<NetworkLinkStatus> {
        self.network_links.iter().map(|(_, value)| value.get_status().clone()).collect()
    }

    pub fn take_link_status_items(&mut self) -> HashMap<CanonicalNetworkId, Arc<NetworkStatusItem<NetworkLinkStatus>>> {
        self.mapped_network_links.clear();
        self.network_links.drain().collect()
    }

    pub fn set_link_status_item(&mut self, id: CanonicalNetworkId, link: Arc<NetworkStatusItem<NetworkLinkStatus>>) {
        id.name().into_iter().for_each(|name| {
            self.mapped_network_links.insert(name.clone(), link.clone());
        });
        self.network_links.insert(id, link);
    }

    pub fn list_interface_status(&self) -> Vec<NetworkInterfaceStatus> {
        self.network_interfaces.iter().map(|(_, value)| value.get_status().clone()).collect()
    }

    pub fn take_interface_status_items(&mut self) -> HashMap<CanonicalNetworkId, Arc<NetworkStatusItem<NetworkInterfaceStatus>>> {
        self.mapped_network_interfaces.clear();
        self.network_interfaces.drain().collect()
    }

    pub fn set_interface_status_item(&mut self, id: CanonicalNetworkId, interface: Arc<NetworkStatusItem<NetworkInterfaceStatus>>) {
        id.name().into_iter().for_each(|name| {
            self.mapped_network_interfaces.insert(name.clone(), interface.clone());
        });
        self.network_interfaces.insert(id, interface);
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct CanonicalNetworkId {
    id: Option<u64>,
    name: Option<String>,
    device: Option<String>,
}
impl CanonicalNetworkId {
    fn new(id: Option<u64>, name: Option<String>, device: Option<String>) -> Self {
        Self { id, name, device }
    }

    fn id(&self) -> Option<u64> {
        self.id
    }

    fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    fn device(&self) -> Option<&String> {
        self.device.as_ref()
    }
}

#[derive(Debug)]
struct NetworkStatusItem<T> {
    id: CanonicalNetworkId,
    status: T,
    refreshed: Instant,
}
impl<T> NetworkStatusItem<T> {
    pub fn new(id: CanonicalNetworkId, status: T) -> NetworkStatusItem<T> {
        NetworkStatusItem { id, refreshed: Instant::now(), status }
    }

    pub fn get_refreshed(&self) -> &Instant {
        &self.refreshed
    }

    pub fn get_id(&self) -> &CanonicalNetworkId {
        &self.id
    }

    pub fn get_status(&self) -> &T {
        &self.status
    }
}

struct InterfaceManagerWorker {
    netlink_message_processor: Arc<NetlinkMessageProcessor>,
}
impl InterfaceManagerWorker {
    pub async fn start(netlink_message_processor: Arc<NetlinkMessageProcessor>, running: Arc<AtomicBool>, netlink_socket: Arc<dyn NetlinkSocket + Send + Sync>, data: Arc<RwLock<InterfaceManagerData>>) {
        let worker = InterfaceManagerWorker { netlink_message_processor };

        worker.poll(&netlink_socket, &data).await;
        tokio::task::spawn(async move {
            let poll_interval = Duration::from_secs(ENTROPY_SCAN_PERIOD_SECONDS);
            let data = data.clone();
            let netlink_socket = netlink_socket.clone();
            let mut interval = tokio::time::interval_at(Instant::now().add(poll_interval), poll_interval);
            interval.tick().await;
            while running.load(Ordering::SeqCst) {
                worker.poll(&netlink_socket, &data).await;
                interval.tick().await;
            };
        });
    }

    pub async fn poll(&self, netlink_socket: &Arc<dyn NetlinkSocket + Send + Sync>, data: &Arc<RwLock<InterfaceManagerData>>) {
        if let Err(e) = &self.try_poll(netlink_socket, data).await {
            error!("Failed to poll interfaces: {:?}", e);
        }
    }

    // This will not debounce interfaces that are deleted and re-created outside this router.
    // However, this is (likely) a deliberate action and will be left out of scope of this router, for now.
    async fn try_poll(&self, netlink_socket: &Arc<dyn NetlinkSocket + Send + Sync>, data: &Arc<RwLock<InterfaceManagerData>>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let link_message = netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default());
        let address_message = netlink_packet_route::RtnlMessage::GetAddress(netlink_packet_route::AddressMessage::default());
        let mut link_data = netlink_socket.send_message(build_default_packet(link_message)).await?;
        let mut network_data = netlink_socket.send_message(build_default_packet(address_message)).await?;

        let mut mapped_devices: HashMap<u64, Arc<NetworkStatusItem<NetworkLinkStatus>>> = HashMap::new();

        let mut mapped_links: HashMap<String, Arc<NetworkStatusItem<NetworkLinkStatus>>> = HashMap::new();
        let mut network_links: HashMap<CanonicalNetworkId, Arc<NetworkStatusItem<NetworkLinkStatus>>> = HashMap::new();
        let mut unmapped_links: HashMap<String, String> = self.netlink_message_processor.get_config().get_network_links().iter().map(|(name, link)| (name.clone(), link.get_device().clone())).collect();

        let mut unmapped_network_interfaces: HashSet<u64> = HashSet::new();
        let mut missing_network_interfaces = HashMap::new();
        let mut network_interfaces: HashMap<CanonicalNetworkId, Arc<NetworkStatusItem<NetworkInterfaceStatus>>> = HashMap::new();
        let link_network_interfaces: HashMap<String, String> = self.netlink_message_processor.get_config().get_network_interfaces().iter().map(|(name, interface)| (interface.get_network_link().clone(), name.clone())).collect();
        
        link_data.drain(..).for_each(|response| {
            if let Some((index, network_link)) = self.netlink_message_processor.process_link_message(response) {
                unmapped_network_interfaces.insert(index);
                let id = CanonicalNetworkId::new(Some(index), network_link.get_name().clone(), Some(network_link.get_device().clone()));
                let link_network_status_item = Arc::new(NetworkStatusItem::new(id.clone(), network_link));

                if let Some(name) = link_network_status_item.get_status().get_name() {
                    unmapped_links.remove(name);
                    mapped_links.insert(name.clone(), link_network_status_item.clone());
                }
                mapped_devices.insert(index, link_network_status_item.clone());
                network_links.insert(id, link_network_status_item.clone());
            };
        });
        unmapped_links.drain().for_each(|(name, device)| {
            let id = CanonicalNetworkId::new(None, Some(name.clone()), Some(device.clone()));
            let network_link_status = NetworkLinkStatus::new(Some(name.clone()), device.clone(), rusty_router_model::NetworkLinkOperationalState::NotFound);
            network_links.insert(id.clone(), Arc::new(NetworkStatusItem::new(id, network_link_status.clone())));
            if let Some(interface_name) = link_network_interfaces.get(&name) {
                let interface_id = CanonicalNetworkId::new(None, Some(interface_name.clone()), Some(device.clone()));
                missing_network_interfaces.insert(interface_id.clone(), Arc::new(NetworkStatusItem::new(interface_id.clone(), NetworkInterfaceStatus::new(Some(interface_name.clone()), vec![], network_link_status))));
            }
        });

        let mut network_addresses: HashMap<u64, Vec<rusty_router_model::IpAddress>> = HashMap::new();
        network_data.drain(..).for_each(|response| {
            if let Some((index, address)) = self.netlink_message_processor.process_address_message(response) {
                let addresses = network_addresses.entry(index).or_insert(vec![]);
                unmapped_network_interfaces.remove(&index);
                addresses.push(address);
            }
        });

        for (network_interface_name, network_interface) in self.netlink_message_processor.get_config().get_network_interfaces() {
            if let Some(network_link) = mapped_links.get(network_interface.get_network_link()) {
                let id = CanonicalNetworkId::new(network_link.get_id().id(), Some(network_interface_name.clone()), network_link.get_id().device().and_then(|device| Some(device.clone())));
                let interface_addresses = network_link.get_id().id().map(|id| network_addresses.remove(&id).map_or_else(|| Vec::new(), |addresses| addresses)).map_or_else(|| Vec::new(), |addresses| addresses);
                network_interfaces.insert(id.clone(), Arc::new(NetworkStatusItem::new(id, NetworkInterfaceStatus::new(Some(network_interface_name.clone()), interface_addresses, network_link.get_status().clone()))));
            } else {
                warn!("Could not match network interface '{}' to a network link '{}'.  Please check the router configuration.", network_interface_name, network_interface.get_network_link());
            }
        }
        for (index, addresses) in network_addresses.drain() {
            if let Some(network_link) = mapped_devices.get(&index) {
                let id = CanonicalNetworkId::new(Some(index), None, Some(network_link.get_status().get_device().clone()));
                network_interfaces.insert(id.clone(), Arc::new(NetworkStatusItem::new(id, NetworkInterfaceStatus::new(None, addresses, network_link.get_status().clone()))));            
            }
        }
        unmapped_network_interfaces.drain().for_each(|index| {
            if let Some(network_link) = mapped_devices.get(&index) {
                let id = CanonicalNetworkId::new(Some(index), None, Some(network_link.get_status().get_device().clone()));
                network_interfaces.insert(id.clone(), Arc::new(NetworkStatusItem::new(id, NetworkInterfaceStatus::new(None, vec![], network_link.get_status().clone()))));            
            }
        });
        missing_network_interfaces.drain().for_each(|(id, interface)| {
            network_interfaces.insert(id, interface);
        });

        let mut data = data.write().await;
        let mut existing_links = data.take_link_status_items();
        let mut existing_interfaces = data.take_interface_status_items();

        let mut notify_links = Vec::new();
        let mut notify_deleted_links = Vec::new();

        let mut notify_interfaces = Vec::new();
        let mut notify_deleted_interfaces = Vec::new();

        network_links.drain().for_each(|(id, link)| {
            let existing_link = existing_links.remove(&id);
            if let Some(existing_link) = existing_link {
                if &existing_link.get_refreshed().add(Duration::from_secs(ENTROPY_HOLD_PERIOD_SECONDS)) < link.get_refreshed() && existing_link.get_status() != link.get_status() {
                    notify_links.push(link.get_status().clone());
                    data.set_link_status_item(id, link);
                } else {
                    data.set_link_status_item(id, existing_link);
                }
            } else {
                notify_links.push(link.get_status().clone());
                data.set_link_status_item(id, link);
            }
        });
        existing_links.drain().for_each(|(_, link)| notify_deleted_links.push(link.get_status().clone()));

        network_interfaces.drain().for_each(|(id, interface)| {
            let existing_interface = existing_interfaces.remove(&id);
            if let Some(existing_interface) = existing_interface {
                if &existing_interface.get_refreshed().add(Duration::from_secs(ENTROPY_HOLD_PERIOD_SECONDS)) < interface.get_refreshed() && existing_interface.get_status() != interface.get_status() {
                    notify_interfaces.push(interface.get_status().clone());
                    data.set_interface_status_item(id, interface);
                } else {
                    data.set_interface_status_item(id, existing_interface);
                }
            } else {
                notify_interfaces.push(interface.get_status().clone());
                data.set_interface_status_item(id, interface);
            }
        });
        existing_interfaces.drain().for_each(|(_, interface)| notify_deleted_interfaces.push(interface.get_status().clone()));

        Ok(())
    }
}

struct InterfaceManagerNetlinkSocketListener {
}
impl InterfaceManagerNetlinkSocketListener {
    pub fn new(_netlink_message_processor: Arc<NetlinkMessageProcessor>, _data: Arc<RwLock<InterfaceManagerData>>) -> InterfaceManagerNetlinkSocketListener {
        InterfaceManagerNetlinkSocketListener { }
    }
}
#[async_trait]
impl NetlinkSocketListener for InterfaceManagerNetlinkSocketListener {
    async fn message_received(&self, _message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) {
        // let mut data = self.data.write().await;
        // if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(_)) = message.payload {
        //     if let Some(link_data) = self.netlink_message_processor.process_link_message(message) {
        //         if let Some(item) = data.get_interface_status_item(link_data.get_index()) {
        //             let nls = NetworkLinkStatus::new(None, link_data.get_name().clone(), rusty_router_model::NetworkLinkOperationalState::Down);
        //             let status = NetworkInterfaceStatus::new(None, item.get_interface_status().get_addresses().clone(), nls);
        //             data.set_interface_status_item(*link_data.get_index(), NetworkInterfaceStatusItem::new(status));
        //         }
        //     }
        // } else if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewAddress(_)) = message.payload {
        //     if let Some(address_data) = process_address_message(message) {
        //         if let Some(item) = data.get_interface_status_item(address_data.get_index()) {
        //             let nls = NetworkLinkStatus::new(None, item.get_interface_status().get_network_link_status().get_device().clone(), rusty_router_model::NetworkLinkOperationalState::Down);
        //             let status = NetworkInterfaceStatus::new(item.get_interface_status().get_network_link_status().get_name().clone(), item.get_interface_status().get_addresses().clone(), nls);
        //             data.set_interface_status_item(*address_data.get_index(), NetworkInterfaceStatusItem::new(status));
        //         }
        //     }
        // } else if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::DelAddress(_)) = message.payload {
        //     if let Some(address_data) = process_address_message(message) {
        //         if let Some(item) = data.get_interface_status_item(address_data.get_index()) {
        //             let nls = NetworkLinkStatus::new(None, item.get_interface_status().get_network_link_status().get_device().clone(), rusty_router_model::NetworkLinkOperationalState::Down);
        //             let status = NetworkInterfaceStatus::new(item.get_interface_status().get_network_link_status().get_name().clone(), item.get_interface_status().get_addresses().clone(), nls);
        //             data.set_interface_status_item(*address_data.get_index(), NetworkInterfaceStatusItem::new(status));
        //         }
        //     }
        // }
    }
}

struct NetlinkMessageProcessor {
    config: Arc<rusty_router_model::Router>,
    device_links: HashMap<String, String>,
}
impl NetlinkMessageProcessor {
    pub fn new(config: Arc<rusty_router_model::Router>) -> NetlinkMessageProcessor {
        let device_links = config.get_network_links().iter().map(|(name, link)| (link.get_device().clone(), name.clone())).collect();
        NetlinkMessageProcessor { config, device_links }
    }

    pub fn get_config(&self) -> &Arc<rusty_router_model::Router> {
        &self.config
    }

    fn process_link_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<(u64, NetworkLinkStatus)> {
        let mut index: Option<u64> = None;
        let mut device: Option<String> = None;
        let mut state = rusty_router_model::NetworkLinkOperationalState::Unknown;
    
        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) = message.payload {
            index = Some(msg.header.index as u64);
            for attribute in msg.nlas.iter() {
                if let nlas::Nla::IfName(ifname) = attribute {
                    device = Some(ifname.clone())
                } else if let nlas::Nla::OperState(operational_state) = attribute {
                    state = match operational_state {
                        nlas::State::Up => rusty_router_model::NetworkLinkOperationalState::Up,
                        nlas::State::Down => rusty_router_model::NetworkLinkOperationalState::Down,
                        _ => rusty_router_model::NetworkLinkOperationalState::Unknown,
                    }
                }
            }
        } else {
            warn!("Netlink data does not contain a payload: {:?}", message)
        }

        index.and_then(|index| device.and_then(|device| Some((index, NetworkLinkStatus::new(self.device_links.get(&device).map(|x| x.clone()), device, state)))))
    }

    fn process_address_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<(u64, rusty_router_model::IpAddress)> {
        let mut index: Option<u64> = None;
        let mut prefix: Option<u64> = None;
        let mut address: Option<String> = None;
        let mut family: Option<rusty_router_model::IpAddressType> = None;
    
        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewAddress(msg)) = message.payload {
            index = Some(msg.header.index as u64);
            prefix = Some(msg.header.prefix_len as u64);
    
            if msg.header.family as u16 == netlink_packet_route::AF_INET {
                for attribute in msg.nlas.iter() {
                    if let netlink_packet_route::address::nlas::Nla::Address(data) = attribute {
                        if data.len() == 4 {
                            family = Some(rusty_router_model::IpAddressType::IpV4);
                            address = Some(Ipv4Addr::from([data[0], data[1], data[2], data[3]]).to_string());
                        }
                    }
                }
            }
            if msg.header.family as u16 == netlink_packet_route::AF_INET6 {
                for attribute in msg.nlas.iter() {
                    if let netlink_packet_route::address::nlas::Nla::Address(data) = attribute {
                        if data.len() == 16 {
                            family = Some(rusty_router_model::IpAddressType::IpV6);
                            address = Some(Ipv6Addr::from([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]).to_string())
                        }
                    }
                }
            }
        } else {
            warn!("Netlink data does not contain a payload: {:?}", message)
        }
    
        family.and_then(|family| address.and_then(|address| prefix.and_then(|prefix| index.and_then(|index| Some((index, rusty_router_model::IpAddress (
            family, address, prefix
        )))))))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::error::Error;
    use std::sync::Arc;

    use crate::netlink::MockNetlinkSocket;
    use crate::netlink::MockNetlinkSocketFactory;

    use super::InterfaceManager;

    #[tokio::test]
    pub async fn test_list_network_interfaces_empty() -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut mock_netlink_socket = MockNetlinkSocket::new();
        let mut mock_netlink_socket_factory = MockNetlinkSocketFactory::new();

        expect_get_link(&mut mock_netlink_socket, vec![]);
        expect_get_address(&mut mock_netlink_socket, vec![]);

        let mock_netlink_socket = Arc::new(mock_netlink_socket);
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| {
            Ok(mock_netlink_socket.clone())
        });
        
        let mock_netlink_socket_factory = Arc::new(mock_netlink_socket_factory);
        InterfaceManager::new(Arc::new(rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new())), mock_netlink_socket_factory).await?;
        Ok(())
    }

    fn expect_get_link(mock_netlink_socket: &mut MockNetlinkSocket, result: Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>) {
        mock_netlink_socket.expect_send_message().times(1).returning(move |msg| {
            assert_ne!(msg.header.sequence_number, 0);
            match msg.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => (),
                _ => assert!(false, "Unexpected parameter"),
            }
            Ok(result.clone())
        });
    }

    fn expect_get_address(mock_netlink_socket: &mut MockNetlinkSocket, result: Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>) {
        mock_netlink_socket.expect_send_message().times(1).returning(move |msg| {
            assert_ne!(msg.header.sequence_number, 0);
            match msg.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => (),
                _ => assert!(false, "Unexpected parameter"),
            }
            Ok(result.clone())
        });

    }
}