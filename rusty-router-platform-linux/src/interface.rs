use std::collections::HashMap;
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

use crate::link::NetlinkRustyRouterLinkStatus;
use crate::netlink::NetlinkSocket;
use crate::netlink::NetlinkSocketFactory;
use crate::netlink::NetlinkSocketListener;
use crate::netlink::build_default_packet;
use crate::route::NetlinkRustyRouterAddressResult;

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
        let data = Arc::new(RwLock::new(InterfaceManagerData::new()));
        let netlink_socket = netlink_socket_factory.create_socket(Box::new(InterfaceManagerNetlinkSocketListener::new(config.clone(), data.clone()))).await?;
        let running = Arc::new(AtomicBool::new(true));
        InterfaceManagerWorker::start(config, running.clone(), netlink_socket.clone(), data.clone()).await;
        Ok(InterfaceManager { running, data })
    }

    pub async fn get_data(&self, name: &String) -> Option<NetworkInterfaceStatus> {
        let data = self.data.read().await;
        if let Some(item) = data.fetch_interface_status(name) {
            return Some(item.clone());
        }
        None
    }
}
impl Drop for InterfaceManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

struct InterfaceManagerData {
    items: HashMap<u64, NetworkInterfaceStatusItem>,
}
impl InterfaceManagerData {
    pub fn new() -> InterfaceManagerData {
        InterfaceManagerData { items: HashMap::new() }
    }

    pub fn get_interface_status_item(&mut self, index: &u64) -> Option<&NetworkInterfaceStatusItem> {
        self.items.get(index)
    }

    pub fn set_interface_status_item(&mut self, index: u64, interface: NetworkInterfaceStatusItem) {
        self.items.insert(index, interface);
    }

    pub fn fetch_interface_status(&self, name: &String) -> Option<NetworkInterfaceStatus> {
        for item in self.items.values() {
            if item.get_interface_status().get_name() == &Some(name.clone()) {
                return Some(item.get_interface_status().clone());
            }
        }
        None
    }
}

struct NetworkInterfaceStatusItem {
    refreshed: Instant,
    interface_status: NetworkInterfaceStatus,
}
impl NetworkInterfaceStatusItem {
    pub fn new(interface_status: NetworkInterfaceStatus) -> NetworkInterfaceStatusItem {
        NetworkInterfaceStatusItem { refreshed: Instant::now(), interface_status }
    }

    pub fn get_refreshed(&self) -> &Instant {
        &self.refreshed
    }

    pub fn get_interface_status(&self) -> &NetworkInterfaceStatus {
        &self.interface_status
    }
}

struct InterfaceManagerWorker {
    _config: Arc<rusty_router_model::Router>,
}
impl InterfaceManagerWorker {
    pub async fn start(config: Arc<rusty_router_model::Router>, running: Arc<AtomicBool>, netlink_socket: Arc<dyn NetlinkSocket + Send + Sync>, data: Arc<RwLock<InterfaceManagerData>>) {
        let worker = InterfaceManagerWorker { _config: config };

        worker.poll(&netlink_socket, &data).await;
        tokio::task::spawn(async move {
            let poll_interval = Duration::from_secs(10);
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

    async fn try_poll(&self, netlink_socket: &Arc<dyn NetlinkSocket + Send + Sync>, data: &Arc<RwLock<InterfaceManagerData>>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let link_message = netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default());
        let address_message = netlink_packet_route::RtnlMessage::GetAddress(netlink_packet_route::AddressMessage::default());
        let link_data = netlink_socket.send_message(build_default_packet(link_message)).await?;
        let mut network_data = netlink_socket.send_message(build_default_packet(address_message)).await?;

        let mut updated_addresses: HashMap<u64, Vec<NetlinkRustyRouterAddressResult>> = HashMap::new();
        network_data.drain(..).for_each(|item| {
            if let Some(address_item) = process_address_message(item) {
                let addresses = updated_addresses.entry(*address_item.get_index()).or_insert(vec![]);
                addresses.push(address_item);
            }
        });

        let mut updated_items: HashMap<u64, NetworkInterfaceStatus> = HashMap::new();
        for item in link_data {
            if let Some(interface) = process_link_message(item) {
                let addresses = updated_addresses.remove(interface.get_index()).get_or_insert(vec![]).drain(..).map(|x| x.address).collect();
                let network_interface_status = NetworkInterfaceStatus::new(None, addresses, NetworkLinkStatus::new(interface.get_name().clone(), None, interface.get_state().clone()));
                updated_items.entry(*interface.get_index()).and_modify(|existing| {
                    warn!("Duplicate entry found {} with a conflicting index {}.  Conflicts with {}.", interface.get_name(), interface.get_index(), existing.get_network_link_status().get_device());
                }).or_insert(network_interface_status);
            }
        }

        let mut data = data.write().await;
        for (index, item) in updated_items.drain() {
            if let Some(interface) = data.get_interface_status_item(&index) {
                if interface.get_refreshed().add(Duration::from_secs(10)) < Instant::now() {
                    data.set_interface_status_item(index, NetworkInterfaceStatusItem::new(item));
                }
            }
        }

        Ok(())
    }
}

struct InterfaceManagerNetlinkSocketListener {
    _config: Arc<rusty_router_model::Router>,
    data: Arc<RwLock<InterfaceManagerData>>,
}
impl InterfaceManagerNetlinkSocketListener {
    pub fn new(config: Arc<rusty_router_model::Router>, data: Arc<RwLock<InterfaceManagerData>>) -> InterfaceManagerNetlinkSocketListener {
        InterfaceManagerNetlinkSocketListener { _config: config, data }
    }
}
#[async_trait]
impl NetlinkSocketListener for InterfaceManagerNetlinkSocketListener {
    async fn message_received(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) {
        let mut data = self.data.write().await;
        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(_)) = message.payload {
            if let Some(link_data) = process_link_message(message) {
                if let Some(item) = data.get_interface_status_item(link_data.get_index()) {
                    let nls = NetworkLinkStatus::new(link_data.get_name().clone(), item.get_interface_status().get_network_link_status().get_name().clone(), rusty_router_model::NetworkLinkOperationalState::Down);
                    let status = NetworkInterfaceStatus::new(item.get_interface_status().get_network_link_status().get_name().clone(), item.get_interface_status().get_addresses().clone(), nls);
                    data.set_interface_status_item(*link_data.get_index(), NetworkInterfaceStatusItem::new(status));
                }
            }
        } else if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewAddress(_)) = message.payload {
            if let Some(address_data) = process_address_message(message) {
                if let Some(item) = data.get_interface_status_item(address_data.get_index()) {
                    let nls = NetworkLinkStatus::new(item.get_interface_status().get_network_link_status().get_device().clone(), item.get_interface_status().get_network_link_status().get_name().clone(), rusty_router_model::NetworkLinkOperationalState::Down);
                    let status = NetworkInterfaceStatus::new(item.get_interface_status().get_network_link_status().get_name().clone(), item.get_interface_status().get_addresses().clone(), nls);
                    data.set_interface_status_item(*address_data.get_index(), NetworkInterfaceStatusItem::new(status));
                }
            }
        } else if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::DelAddress(_)) = message.payload {
            if let Some(address_data) = process_address_message(message) {
                if let Some(item) = data.get_interface_status_item(address_data.get_index()) {
                    let nls = NetworkLinkStatus::new(item.get_interface_status().get_network_link_status().get_device().clone(), item.get_interface_status().get_network_link_status().get_name().clone(), rusty_router_model::NetworkLinkOperationalState::Down);
                    let status = NetworkInterfaceStatus::new(item.get_interface_status().get_network_link_status().get_name().clone(), item.get_interface_status().get_addresses().clone(), nls);
                    data.set_interface_status_item(*address_data.get_index(), NetworkInterfaceStatusItem::new(status));
                }
            }
        }
    }
}

fn process_link_message(message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<NetlinkRustyRouterLinkStatus> {
    let mut index: Option<u64> = None;
    let mut name: Option<String> = None;
    let mut state = rusty_router_model::NetworkLinkOperationalState::Unknown;

    if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) = message.payload {
        index = Some(msg.header.index as u64);
        for attribute in msg.nlas.iter() {
            if let nlas::Nla::IfName(ifname) = attribute {
                name = Some(ifname.clone())
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

    index.and_then(|index| name.and_then(|name| Some(NetlinkRustyRouterLinkStatus::new(index, name, state))))
}

fn process_address_message(message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<NetlinkRustyRouterAddressResult> {
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

    family.and_then(|family| address.and_then(|address| prefix.and_then(|prefix| index.and_then(|index| Some(NetlinkRustyRouterAddressResult{ index, address: rusty_router_model::IpAddress (
        family, address, prefix
    )})))))
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