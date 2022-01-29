mod database;
mod model;

use crate::netlink::NetlinkSocket;
use crate::netlink::NetlinkSocketFactory;
use crate::netlink::NetlinkSocketListener;

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, IpAddr};
use std::ops::Add;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{sync::Arc, error::Error};

use log::{error, warn};
use async_trait::async_trait;
use netlink_packet_route::link::nlas;
use rusty_router_model::{NetworkInterfaceStatus, NetworkLinkStatus, Router, NetworkLinkOperationalState};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::netlink::build_default_packet;

use self::database::InterfaceManagerDatabase;
use self::model::{NetworkStatusItem, CanonicalNetworkId};

const ENTROPY_HOLD_PERIOD_SECONDS: u64 = 5;
const ENTROPY_SCAN_PERIOD_SECONDS: u64 = 10;

/**
 * Two mechanisms exist to maintain a list of network interface information.
 * A listener will subscribe to all networking events to ensure we have the latest information, but this can be lossy.
 * An anti-entrophy scan is carried out periodically to ensure missing or incomplete information is captured.
 * 
 * TODO Validate garbage config like reused devices, reused network links per interface or non-existant links.
 */
pub struct InterfaceManager {
    running: Arc<AtomicBool>,
    database: Arc<RwLock<InterfaceManagerDatabase>>,
}
impl InterfaceManager {
    pub async fn new(config: Arc<Router>, netlink_socket_factory: Arc<dyn NetlinkSocketFactory + Send + Sync>) -> Result<InterfaceManager, Box<dyn Error + Send + Sync>> {
        let running = Arc::new(AtomicBool::new(true));
        let database = Arc::new(RwLock::new(InterfaceManagerDatabase::new()));
        let interface_manaer = InterfaceManager { running: running.clone(), database: database.clone() };

        // Perform these operations are creating an interface manager to ensure running is set to faule upon failure.
        let netlink_message_processor = Arc::new(NetlinkMessageProcessor::new(config.clone()));
        let netlink_socket = netlink_socket_factory.create_socket(Box::new(InterfaceManagerNetlinkSocketListener::new(netlink_message_processor.clone(), database.clone()))).await?;
        InterfaceManagerWorker::start(config.clone(), running.clone(), database.clone(), netlink_socket.clone(), netlink_message_processor.clone()).await;
        Ok(interface_manaer)
    }

    pub async fn list_network_links(&self) -> Vec<NetworkLinkStatus> {
        self.database.read().await.list_link_status()
    }

    pub async fn list_network_interfaces(&self) -> Vec<NetworkInterfaceStatus> {
        self.database.read().await.list_interface_status()
    }
}
impl Drop for InterfaceManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

struct InterfaceManagerWorker {
    config: Arc<Router>,
    running: Arc<AtomicBool>,
    database: Arc<RwLock<InterfaceManagerDatabase>>,
    netlink_socket: Arc<dyn NetlinkSocket + Send + Sync>,
    netlink_message_processor: Arc<NetlinkMessageProcessor>,
}
impl InterfaceManagerWorker {
    pub async fn start(config: Arc<Router>, running: Arc<AtomicBool>, database: Arc<RwLock<InterfaceManagerDatabase>>, netlink_socket: Arc<dyn NetlinkSocket + Send + Sync>, netlink_message_processor: Arc<NetlinkMessageProcessor>) {
        let worker = InterfaceManagerWorker { config, running, database, netlink_socket, netlink_message_processor };

        worker.poll().await;
        tokio::task::spawn(async move {
            let poll_interval = Duration::from_secs(ENTROPY_SCAN_PERIOD_SECONDS);
            let mut interval = tokio::time::interval_at(Instant::now().add(poll_interval), poll_interval);
            interval.tick().await;
            while worker.running.load(Ordering::SeqCst) {
                worker.poll().await;
                interval.tick().await;
            };
        });
    }

    async fn poll(&self) {
        if let Err(e) = &self.try_poll().await {
            error!("Failed to poll interfaces: {:?}", e);
        }
    }

    // This will not debounce interfaces that are deleted and re-created outside this router.
    // However, this is (likely) a deliberate action and will be left out of scope of this router, for now.
    async fn try_poll(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let link_message = netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default());
        let address_message = netlink_packet_route::RtnlMessage::GetAddress(netlink_packet_route::AddressMessage::default());
        let mut link_data = self.netlink_socket.send_message(build_default_packet(link_message)).await?;
        let mut network_data = self.netlink_socket.send_message(build_default_packet(address_message)).await?;

        let mut mapped_devices: HashMap<u64, NetworkStatusItem<NetworkLinkStatus>> = HashMap::new();

        let mut mapped_links: HashMap<String, NetworkStatusItem<NetworkLinkStatus>> = HashMap::new();
        let mut network_links: HashMap<CanonicalNetworkId, NetworkStatusItem<NetworkLinkStatus>> = HashMap::new();
        let mut unmapped_links: HashMap<String, String> = self.config.get_network_links().iter().map(|(name, link)| (name.clone(), link.get_device().clone())).collect();

        let mut missing_network_interfaces = HashMap::new();
        let mut network_interfaces: HashMap<CanonicalNetworkId, NetworkStatusItem<NetworkInterfaceStatus>> = HashMap::new();
        let link_network_interfaces: HashMap<String, String> = self.config.get_network_interfaces().iter().map(|(name, interface)| (interface.get_network_link().clone(), name.clone())).collect();
        
        link_data.drain(..).for_each(|response| {
            if let Some((index, network_link)) = self.netlink_message_processor.process_link_message(response) {
                let id = CanonicalNetworkId::new(Some(index), network_link.get_name().clone(), Some(network_link.get_device().clone()));
                let link_network_status_item = NetworkStatusItem::new(id.clone(), network_link);

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
            network_links.insert(id.clone(), NetworkStatusItem::new(id, network_link_status.clone()));
            if let Some(interface_name) = link_network_interfaces.get(&name) {
                let interface_id = CanonicalNetworkId::new(None, Some(interface_name.clone()), Some(device.clone()));
                missing_network_interfaces.insert(interface_id.clone(), NetworkStatusItem::new(interface_id.clone(), NetworkInterfaceStatus::new(Some(interface_name.clone()), vec![], network_link_status)));
            }
        });

        let mut network_addresses: HashMap<u64, Vec<rusty_router_model::IpAddress>> = HashMap::new();
        network_data.drain(..).for_each(|response| {
            if let Some((index, address)) = self.netlink_message_processor.process_address_message(response) {
                let addresses = network_addresses.entry(index).or_insert(vec![]);
                addresses.push(address);
            }
        });

        for (network_interface_name, network_interface) in self.config.get_network_interfaces() {
            if let Some(network_link) = mapped_links.get(network_interface.get_network_link()) {
                let id = CanonicalNetworkId::new(network_link.get_id().id(), Some(network_interface_name.clone()), network_link.get_id().device().and_then(|device| Some(device.clone())));
                let mut interface_addresses = network_link.get_id().id().map(|id| network_addresses.remove(&id).map_or_else(|| Vec::new(), |addresses| addresses)).map_or_else(|| Vec::new(), |addresses| addresses);
                interface_addresses.sort();
                network_interfaces.insert(id.clone(), NetworkStatusItem::new(id, NetworkInterfaceStatus::new(Some(network_interface_name.clone()), interface_addresses, network_link.get_status().clone())));
            } else if let Some(network_link) = self.config.get_network_links().get(network_interface.get_network_link()) {
                let id = CanonicalNetworkId::new(None, Some(network_interface_name.clone()), Some(network_link.get_device().clone()));
                network_interfaces.insert(id.clone(), NetworkStatusItem::new(id, NetworkInterfaceStatus::new(Some(network_interface_name.clone()), vec![], NetworkLinkStatus::new(Some(network_interface.get_network_link().clone()), network_link.get_device().clone(), NetworkLinkOperationalState::Misconfigured))));
            } else {
                let id = CanonicalNetworkId::new(None, Some(network_interface_name.clone()), None);
                network_interfaces.insert(id.clone(), NetworkStatusItem::new(id, NetworkInterfaceStatus::new(Some(network_interface_name.clone()), vec![], NetworkLinkStatus::new(Some(network_interface.get_network_link().clone()), String::new(), NetworkLinkOperationalState::Misconfigured))));
            }
        }
        for (index, mut addresses) in network_addresses.drain() {
            if let Some(network_link) = mapped_devices.get(&index) {
                addresses.sort();
                let id = CanonicalNetworkId::new(Some(index), None, Some(network_link.get_status().get_device().clone()));
                network_interfaces.insert(id.clone(), NetworkStatusItem::new(id, NetworkInterfaceStatus::new(None, addresses, network_link.get_status().clone())));            
            }
        }
        missing_network_interfaces.drain().for_each(|(id, interface)| {
            network_interfaces.insert(id, interface);
        });

        let mut database = self.database.write().await;

        let mut notify_links = Vec::new();
        let mut notify_deleted_links = Vec::new();
        let mut established_links : Vec<NetworkStatusItem<NetworkLinkStatus>> = Vec::new();

        network_links.drain().into_iter().for_each(|(id, link)| {
            let mut deleted_items = Vec::new();
            database.remove_link_status_item(&id, &mut deleted_items);
            deleted_items.into_iter().find(|item| item.get_id() == &id).or_else(|| {
                notify_links.push(link.clone());
                established_links.push(link.clone());
                None
            }).into_iter().for_each(|item| {
                if &item.get_refreshed().add(Duration::from_secs(ENTROPY_HOLD_PERIOD_SECONDS)) < link.get_refreshed() && item.get_status() != link.get_status() {
                    notify_links.push(link.clone());
                    established_links.push(link.clone());
                } else {
                    established_links.push(item);
                }
            });
        });

        database.take_link_status_items().into_iter().for_each(|item| {
            notify_deleted_links.push(item);
        });
        established_links.drain(..).for_each(|item| {
            database.set_link_status_item(item);
        });

        let mut notify_interfaces = Vec::new();
        let mut notify_deleted_interfaces = Vec::new();
        let mut established_interfaces : Vec<NetworkStatusItem<NetworkInterfaceStatus>> = Vec::new();

        network_interfaces.drain().into_iter().for_each(|(id, interface)| {
            let mut deleted_items = Vec::new();
            database.remove_interface_status_item(&id, &mut deleted_items);
            deleted_items.into_iter().find(|item| item.get_id() == &id).or_else(|| {
                notify_interfaces.push(interface.clone());
                established_interfaces.push(interface.clone());
                None
            }).into_iter().for_each(|item| {
                if &item.get_refreshed().add(Duration::from_secs(ENTROPY_HOLD_PERIOD_SECONDS)) < interface.get_refreshed() && item.get_status() != interface.get_status() {
                    notify_interfaces.push(interface.clone());
                    established_interfaces.push(interface.clone());
                } else {
                    established_interfaces.push(item);
                }
            });
        });
        database.take_interface_status_items().into_iter().for_each(|item| {
            notify_deleted_interfaces.push(NetworkStatusItem::new(
                item.get_id().clone(), NetworkInterfaceStatus::new(
                    item.get_status().get_name().clone(), 
                    item.get_status().get_addresses().clone(),
                    NetworkLinkStatus::new(
                        item.get_status().get_network_link_status().get_name().clone(),
                        item.get_status().get_network_link_status().get_device().clone(),
                        NetworkLinkOperationalState::NotFound
                    )
                )
            ));
        });
        established_interfaces.drain(..).for_each(|item| {
            database.set_interface_status_item(item);
        });
        drop(database);

        // TODO Notify

        Ok(())
    }
}

struct InterfaceManagerNetlinkSocketListener {
    database: Arc<RwLock<InterfaceManagerDatabase>>,
    netlink_message_processor: Arc<NetlinkMessageProcessor>,
}
impl InterfaceManagerNetlinkSocketListener {
    pub fn new(netlink_message_processor: Arc<NetlinkMessageProcessor>, data: Arc<RwLock<InterfaceManagerDatabase>>) -> InterfaceManagerNetlinkSocketListener {
        InterfaceManagerNetlinkSocketListener { netlink_message_processor, database: data }
    }
}
#[async_trait]
impl NetlinkSocketListener for InterfaceManagerNetlinkSocketListener {
    async fn message_received(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) {
        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(_)) = message.payload {
            if let Some((index, link)) = self.netlink_message_processor.process_link_message(message) {
                let mut database = self.database.write().await;
                let id = CanonicalNetworkId::new(Some(index), link.get_name().clone(), Some(link.get_device().clone()));
                let link = NetworkStatusItem::new(id.clone(), link.clone());
                database.set_link_status_item(link.clone()); // Ignore deleted links

                let mut updated_interface = None;
                database.get_interface_status_item_by_device_index(&index).into_iter().find(|interface| interface.get_status().get_network_link_status().get_device() == link.get_status().get_device()).into_iter().for_each(|interface| {
                    let id = CanonicalNetworkId::new(Some(index), interface.get_status().get_name().clone(), Some(interface.get_status().get_network_link_status().get_device().clone()));
                    updated_interface = Some(NetworkStatusItem::new(id, NetworkInterfaceStatus::new(interface.get_status().get_name().clone(), interface.get_status().get_addresses().clone(), link.get_status().clone())));
                });
                updated_interface.iter().for_each(|interface| {
                    database.set_interface_status_item(interface.clone());
                });
                
                // TODO Notify
            }
        } else if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::DelLink(_)) = message.payload {
            if let Some((index, link)) = self.netlink_message_processor.process_link_message(message) {
                let mut database = self.database.write().await;

                let mut deleted_items = Vec::new();
                let id = CanonicalNetworkId::new(None, link.get_name().clone(), Some(link.get_device().clone()));
                let link = if let Some(_) = id.name() {
                    let link = NetworkStatusItem::new(id.clone(), NetworkLinkStatus::new(link.get_name().clone(), link.get_device().clone(), NetworkLinkOperationalState::NotFound));
                    database.set_link_status_item(link.clone()); // Ignore deleted links
                    link
                    // TODO Notify
                } else {
                    database.remove_link_status_item(&id, &mut deleted_items);
                    NetworkStatusItem::new(id.clone(), link)
                };

                let mut updated_interface = None;
                let mut deleted_interfaces = Vec::new();
                database.get_interface_status_item_by_device_index(&index).into_iter().find(|interface| interface.get_status().get_network_link_status().get_device() == link.get_status().get_device()).into_iter().for_each(|interface| {
                    let id = CanonicalNetworkId::new(None, interface.get_status().get_name().clone(), Some(interface.get_status().get_network_link_status().get_device().clone()));
                    updated_interface = Some(NetworkStatusItem::new(id, NetworkInterfaceStatus::new(interface.get_status().get_name().clone(), interface.get_status().get_addresses().clone(), link.get_status().clone())));
                });
                updated_interface.iter().for_each(|interface| {
                    if let Some(_) = interface.get_id().name() {
                        database.set_interface_status_item(interface.clone());
                    } else {
                        database.remove_interface_status_item(interface.get_id(), &mut deleted_interfaces);
                    }
                });
            }
        } else if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewAddress(_)) = message.payload {
            if let Some((index, address)) = self.netlink_message_processor.process_address_message(message) {
                let mut database = self.database.write().await;
                if let Some(interface) = database.get_interface_status_item_by_device_index(&index) {
                    let mut addresses = interface.get_status().get_addresses().clone();
                    addresses.push(address);
                    addresses.sort();

                    let id = interface.get_id().clone();
                    let updated_interface = NetworkInterfaceStatus::new(interface.get_status().get_name().clone(), addresses, interface.get_status().get_network_link_status().clone());
                    database.set_interface_status_item(NetworkStatusItem::new(id, updated_interface));
                }
                // TODO Notify
            }
        } else if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::DelAddress(_)) = message.payload {
            if let Some((index, address)) = self.netlink_message_processor.process_address_message(message) {
                let mut database = self.database.write().await;
                if let Some(interface) = database.get_interface_status_item_by_device_index(&index) {
                    let mut addresses = interface.get_status().get_addresses().clone();
                    if let Some(index) = addresses.iter().position(|x| x == &address) {
                        addresses.remove(index);
                    }

                    let id = interface.get_id().clone();
                    let updated_interface = NetworkInterfaceStatus::new(interface.get_status().get_name().clone(), addresses, interface.get_status().get_network_link_status().clone());
                    database.set_interface_status_item(NetworkStatusItem::new(id, updated_interface));
                }
                // TODO Notify
            }
        }
    }
}

struct NetlinkMessageProcessor {
    device_links: HashMap<String, String>,
}
impl NetlinkMessageProcessor {
    pub fn new(config: Arc<Router>) -> NetlinkMessageProcessor {
        let device_links = config.get_network_links().iter().map(|(name, link)| (link.get_device().clone(), name.clone())).collect();
        NetlinkMessageProcessor { device_links }
    }

    fn process_link_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<(u64, NetworkLinkStatus)> {
        let mut device: Option<String> = None;
        let mut state = rusty_router_model::NetworkLinkOperationalState::Unknown;

        let msg = match message.payload {
            netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) => msg,
            netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::DelLink(msg)) => msg,
            _ => {
                warn!("Netlink data does not contain a payload: {:?}", message);
                return None
            },
        };

        let index = msg.header.index as u64;
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
        device.and_then(|device| Some((index, NetworkLinkStatus::new(self.device_links.get(&device).map(|x| x.clone()), device, state))))
    }

    fn process_address_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<(u64, rusty_router_model::IpAddress)> {
        let mut address: Option<IpAddr> = None;
 
        let msg = match message.payload {
            netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewAddress(msg)) => msg,
            netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::DelAddress(msg)) => msg,
            _ => {
                warn!("Netlink data does not contain a payload: {:?}", message);
                return None
            }
        };

        let index = msg.header.index as u64;
        // Here we trust the prefix from the OS.  Validation that is too strict here could lead to compatibility issues.
        let prefix = msg.header.prefix_len as u64;
    
        if msg.header.family as u16 == netlink_packet_route::AF_INET {
            for attribute in msg.nlas.iter() {
                if let netlink_packet_route::address::nlas::Nla::Address(data) = attribute {
                    if data.len() == 4 {
                        address = Some(IpAddr::V4(Ipv4Addr::from([data[0], data[1], data[2], data[3]])));
                    }
                }
            }
        }
        if msg.header.family as u16 == netlink_packet_route::AF_INET6 {
            for attribute in msg.nlas.iter() {
                if let netlink_packet_route::address::nlas::Nla::Address(data) = attribute {
                    if data.len() == 16 {
                        address = Some(IpAddr::V6(Ipv6Addr::from([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]])))
                    }
                }
            }
        }
    
        address.and_then(|address| Some((index, rusty_router_model::IpAddress (
            address, prefix
        ))))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::error::Error;
    use std::net::IpAddr;
    use std::ops::Sub;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tokio::time::Instant;

    use netlink_packet_core::NetlinkHeader;
    use netlink_packet_core::NetlinkMessage;
    use netlink_packet_core::NetlinkPayload;
    use netlink_packet_route::AddressHeader;
    use netlink_packet_route::AddressMessage;
    use netlink_packet_route::LinkHeader;
    use netlink_packet_route::LinkMessage;
    use netlink_packet_route::RtnlMessage;
    use netlink_packet_route::link::nlas::State;
    use rand::random;
    use rusty_router_model::IpAddress;
    use rusty_router_model::NetworkInterface;
    use rusty_router_model::NetworkInterfaceStatus;
    use rusty_router_model::NetworkLink;
    use rusty_router_model::NetworkLinkOperationalState;
    use rusty_router_model::NetworkLinkStatus;
    use rusty_router_model::NetworkLinkType;
    use rusty_router_model::Router;
    use tokio::sync::RwLock;

    use crate::interface::CanonicalNetworkId;
    use crate::interface::NetworkStatusItem;
    use crate::interface::database::InterfaceManagerDatabase;
    use crate::netlink::MockNetlinkSocket;
    use crate::netlink::MockNetlinkSocketFactory;
    use crate::netlink::NetlinkSocketListener;

    use super::InterfaceManager;
    use super::InterfaceManagerNetlinkSocketListener;
    use super::InterfaceManagerWorker;
    use super::NetlinkMessageProcessor;

    #[tokio::test]
    pub async fn test_process_link_message() -> Result<(), Box<dyn Error + Send + Sync>> {
        let netlink_header = NetlinkHeader { sequence_number: random(), flags: random(), port_number: random(), length: random(), message_type: random() };

        let config = Arc::new(Router::new(HashMap::new(), HashMap::new(), HashMap::new()));
        let subject = NetlinkMessageProcessor::new(config);

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: random(), link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![]
        })) }) == None);

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: 10, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice"))]
        })) }) == Some((10, NetworkLinkStatus::new(None, String::from("SomeDevice"), rusty_router_model::NetworkLinkOperationalState::Unknown))));

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: 10, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice"))]
        })) }) == Some((10, NetworkLinkStatus::new(None, String::from("SomeDevice"), rusty_router_model::NetworkLinkOperationalState::Unknown))));

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: 15, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")), netlink_packet_route::link::nlas::Nla::OperState(State::Up)]
        })) }) == Some((15, NetworkLinkStatus::new(None, String::from("SomeDevice1"), rusty_router_model::NetworkLinkOperationalState::Up))));

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::DelLink(LinkMessage {
            header: LinkHeader { index: 15, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")), netlink_packet_route::link::nlas::Nla::OperState(State::Other(100))]
        })) }) == Some((15, NetworkLinkStatus::new(None, String::from("SomeDevice1"), rusty_router_model::NetworkLinkOperationalState::Unknown))));

        // This one logs an error message and continues.  There is nothing a user can really do here.  It really should not be possible to reach.
        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::GetLink(LinkMessage {
            header: LinkHeader { index: 15, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")), netlink_packet_route::link::nlas::Nla::OperState(State::Other(100))]
        })) }) == None);

        let config = Arc::new(Router::new(vec![
            (String::from("NetworkLink1"), NetworkLink::new(String::from("Device1"), NetworkLinkType::GenericInterface)),
            (String::from("NetworkLink2"), NetworkLink::new(String::from("Device2"), NetworkLinkType::GenericInterface)),
            (String::from("NetworkLink3"), NetworkLink::new(String::from("Device3"), NetworkLinkType::GenericInterface)),
        ].drain(..).collect(), HashMap::new(), HashMap::new()));
        let subject = NetlinkMessageProcessor::new(config);

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: random(), link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![]
        })) }) == None);

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: 15, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")), netlink_packet_route::link::nlas::Nla::OperState(State::Up)]
        })) }) == Some((15, NetworkLinkStatus::new(None, String::from("SomeDevice1"), rusty_router_model::NetworkLinkOperationalState::Up))));

        assert!(subject.process_link_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: 20, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![netlink_packet_route::link::nlas::Nla::IfName(String::from("Device2")), netlink_packet_route::link::nlas::Nla::OperState(State::Down)]
        })) }) == Some((20, NetworkLinkStatus::new(Some(String::from("NetworkLink2")), String::from("Device2"), rusty_router_model::NetworkLinkOperationalState::Down))));

        Ok(())
    }

    #[tokio::test]
    pub async fn test_process_address_message() -> Result<(), Box<dyn Error + Send + Sync>> {
        let netlink_header = NetlinkHeader { sequence_number: random(), flags: random(), port_number: random(), length: random(), message_type: random() };

        let config = Arc::new(Router::new(HashMap::new(), HashMap::new(), HashMap::new()));
        let subject = NetlinkMessageProcessor::new(config);

        assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
            header: AddressHeader { index: random(), flags: random(), family: random(), prefix_len: random(), scope: random() },
            nlas: vec![]
        })) }) == None);

        assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
            header: AddressHeader { index: 10, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 20, scope: random() },
            nlas: vec![netlink_packet_route::address::nlas::Nla::Address(vec![1, 2, 3, 4])]
        })) }) == Some((10, IpAddress::new(IpAddr::from_str("1.2.3.4")?, 20))));

        assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::DelAddress(AddressMessage {
            header: AddressHeader { index: 15, flags: random(), family: netlink_packet_route::AF_INET6 as u8, prefix_len: 56, scope: random() },
            nlas: vec![netlink_packet_route::address::nlas::Nla::Address(vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1])]
        })) }) == Some((15, IpAddress::new(IpAddr::from_str("100f:0e0d:0c0b:0a09:0807:0605:0403:0201")?, 56))));

        assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::GetAddress(AddressMessage {
            header: AddressHeader { index: 15, flags: random(), family: netlink_packet_route::AF_INET6 as u8, prefix_len: 56, scope: random() },
            nlas: vec![netlink_packet_route::address::nlas::Nla::Address(vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1])]
        })) }) == None);

        Ok(())
    }

    #[tokio::test]
    pub async fn test_interface_manager_worker_links() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = Arc::new(Router::new(vec![
            (String::from("SomeLink2"), NetworkLink::new(String::from("SomeDevice2"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink3"), NetworkLink::new(String::from("SomeDevice3"), NetworkLinkType::GenericInterface)),
        ].drain(..).collect(), HashMap::new(), HashMap::new()));

        let mut mock_netlink_socket = MockNetlinkSocket::new();
        let netlink_message_processor = Arc::new(NetlinkMessageProcessor::new(config.clone()));

        mock_netlink_socket.expect_send_message().withf(|message| {
            match message.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => true,
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => true,
                _ => false,
            }
        }).returning(|input| {
            match input.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => {
                    let netlink_header = NetlinkHeader { sequence_number: input.header.sequence_number, flags: random(), port_number: random(), length: random(), message_type: random() };

                    let iface1 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 100, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface2 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 101, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice2")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };

                    Ok(vec![iface1, iface2])
                },
                _ => Ok(vec![]),
            }
        });

        let running = Arc::new(AtomicBool::new(false));
        let database = Arc::new(RwLock::new(InterfaceManagerDatabase::new()));
        let subject = InterfaceManagerWorker { config, running: running.clone(), database: database.clone(), netlink_socket: Arc::new(mock_netlink_socket), netlink_message_processor };

        subject.poll().await;
        running.store(false, Ordering::SeqCst);

        assert_eq!(database.read().await.list_link_status(), vec![
            NetworkLinkStatus::new(None, String::from("SomeDevice1"), NetworkLinkOperationalState::Up),
            NetworkLinkStatus::new(Some(String::from("SomeLink2")), String::from("SomeDevice2"), NetworkLinkOperationalState::Up),
            NetworkLinkStatus::new(Some(String::from("SomeLink3")), String::from("SomeDevice3"), NetworkLinkOperationalState::NotFound),
        ]);

        Ok(())
    }

    #[tokio::test]
    pub async fn test_interface_manager_worker_interfaces() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = Arc::new(Router::new(vec![
            (String::from("SomeLink2"), NetworkLink::new(String::from("SomeDevice2"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink3"), NetworkLink::new(String::from("SomeDevice3"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink5"), NetworkLink::new(String::from("SomeDevice5"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink6"), NetworkLink::new(String::from("SomeDevice6"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink7"), NetworkLink::new(String::from("SomeDevice7"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink8"), NetworkLink::new(String::from("SomeDevice8"), NetworkLinkType::GenericInterface)),
        ].drain(..).collect(), vec![
            (String::from("SomeInterface4"), NetworkInterface::new(None, String::from("SomeLink4"), vec![])),
            (String::from("SomeInterface5"), NetworkInterface::new(None, String::from("SomeLink5"), vec![])),
            (String::from("SomeInterface6"), NetworkInterface::new(None, String::from("SomeLink6"), vec![])),
            (String::from("SomeInterface7"), NetworkInterface::new(None, String::from("SomeLink7"), vec![])),
        ].drain(..).collect(), HashMap::new()));

        let mut mock_netlink_socket = MockNetlinkSocket::new();
        let netlink_message_processor = Arc::new(NetlinkMessageProcessor::new(config.clone()));

        mock_netlink_socket.expect_send_message().withf(|message| {
            match message.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => true,
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => true,
                _ => false,
            }
        }).returning(|input| {
            match input.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => {
                    let netlink_header = NetlinkHeader { sequence_number: input.header.sequence_number, flags: random(), port_number: random(), length: random(), message_type: random() };

                    let iface1 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 100, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface2 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 101, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice2")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface5 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 105, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice5")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Down),
                        ]
                    })) };
                    let iface7 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 107, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice7")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface8 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 108, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice8")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };

                    Ok(vec![iface1, iface2, iface5, iface7, iface8])
                },
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => {
                    let netlink_header = NetlinkHeader { sequence_number: input.header.sequence_number, flags: random(), port_number: random(), length: random(), message_type: random() };

                    let address7 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
                        header: AddressHeader { index: 107, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 20, scope: random() },
                        nlas: vec![
                            netlink_packet_route::address::nlas::Nla::Address(vec![1, 2, 3, 4]),
                        ]
                    })) };
                    let address8 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
                        header: AddressHeader { index: 108, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 32, scope: random() },
                        nlas: vec![
                            netlink_packet_route::address::nlas::Nla::Address(vec![2, 3, 4, 5]),
                        ]
                    })) };

                    Ok(vec![address7, address8])
                },
                _ => Ok(vec![]),
            }
        });

        let running = Arc::new(AtomicBool::new(false));
        let database = Arc::new(RwLock::new(InterfaceManagerDatabase::new()));
        let subject = InterfaceManagerWorker { config, running: running.clone(), database: database.clone(), netlink_socket: Arc::new(mock_netlink_socket), netlink_message_processor };

        subject.poll().await;
        running.store(false, Ordering::SeqCst);

        assert_eq!(database.read().await.list_link_status(), vec![
            NetworkLinkStatus::new(None, String::from("SomeDevice1"), NetworkLinkOperationalState::Up),
            NetworkLinkStatus::new(Some(String::from("SomeLink2")), String::from("SomeDevice2"), NetworkLinkOperationalState::Up),
            NetworkLinkStatus::new(Some(String::from("SomeLink3")), String::from("SomeDevice3"), NetworkLinkOperationalState::NotFound),
            NetworkLinkStatus::new(Some(String::from("SomeLink5")), String::from("SomeDevice5"), NetworkLinkOperationalState::Down),
            NetworkLinkStatus::new(Some(String::from("SomeLink6")), String::from("SomeDevice6"), NetworkLinkOperationalState::NotFound),
            NetworkLinkStatus::new(Some(String::from("SomeLink7")), String::from("SomeDevice7"), NetworkLinkOperationalState::Up),
            NetworkLinkStatus::new(Some(String::from("SomeLink8")), String::from("SomeDevice8"), NetworkLinkOperationalState::Up),
        ]);
        assert_eq!(database.read().await.list_interface_status(), vec![
            NetworkInterfaceStatus::new(None, vec![IpAddress::new(IpAddr::from_str("2.3.4.5")?, 32)], NetworkLinkStatus::new(Some(String::from("SomeLink8")), String::from("SomeDevice8"), NetworkLinkOperationalState::Up)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface4")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink4")), String::from(""), NetworkLinkOperationalState::Misconfigured)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface5")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink5")), String::from("SomeDevice5"), NetworkLinkOperationalState::Down)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface6")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink6")), String::from("SomeDevice6"), NetworkLinkOperationalState::NotFound)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface7")), vec![IpAddress::new(IpAddr::from_str("1.2.3.4")?, 20)], NetworkLinkStatus::new(Some(String::from("SomeLink7")), String::from("SomeDevice7"), NetworkLinkOperationalState::Up)),
        ]);

        Ok(())
    }

    #[tokio::test]
    pub async fn test_interface_manager_worker_interfaces_update() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = Arc::new(Router::new(vec![
            (String::from("SomeLink2"), NetworkLink::new(String::from("SomeDevice2"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink3"), NetworkLink::new(String::from("SomeDevice3"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink5"), NetworkLink::new(String::from("SomeDevice5"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink6"), NetworkLink::new(String::from("SomeDevice6"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink7"), NetworkLink::new(String::from("SomeDevice7"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink8"), NetworkLink::new(String::from("SomeDevice8"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink9"), NetworkLink::new(String::from("SomeDevice9"), NetworkLinkType::GenericInterface)),
        ].drain(..).collect(), vec![
            (String::from("SomeInterface4"), NetworkInterface::new(None, String::from("SomeLink4"), vec![])),
            (String::from("SomeInterface5"), NetworkInterface::new(None, String::from("SomeLink5"), vec![])),
            (String::from("SomeInterface6"), NetworkInterface::new(None, String::from("SomeLink6"), vec![])),
            (String::from("SomeInterface7"), NetworkInterface::new(None, String::from("SomeLink7"), vec![])),
            (String::from("SomeInterface9"), NetworkInterface::new(None, String::from("SomeLink9"), vec![])),
        ].drain(..).collect(), HashMap::new()));

        let mut mock_netlink_socket = MockNetlinkSocket::new();
        let netlink_message_processor = Arc::new(NetlinkMessageProcessor::new(config.clone()));

        mock_netlink_socket.expect_send_message().withf(|message| {
            match message.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => true,
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => true,
                _ => false,
            }
        }).times(2).returning(|input| {
            match input.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => {
                    let netlink_header = NetlinkHeader { sequence_number: input.header.sequence_number, flags: random(), port_number: random(), length: random(), message_type: random() };

                    let iface1 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 100, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface2 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 101, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice2")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Down),
                        ]
                    })) };
                    let iface5 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 105, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice5")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Down),
                        ]
                    })) };
                    let iface7 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 107, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice7")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface8 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 108, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice8")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface9 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 109, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice9")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Down),
                        ]
                    })) };
                    let iface10 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 110, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDeviceA")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Down),
                        ]
                    })) };

                    Ok(vec![iface1, iface2, iface5, iface7, iface8, iface9, iface10])
                },
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => {
                    let netlink_header = NetlinkHeader { sequence_number: input.header.sequence_number, flags: random(), port_number: random(), length: random(), message_type: random() };

                    let address7 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
                        header: AddressHeader { index: 107, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 20, scope: random() },
                        nlas: vec![
                            netlink_packet_route::address::nlas::Nla::Address(vec![1, 2, 3, 4]),
                        ]
                    })) };
                    let address8 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
                        header: AddressHeader { index: 108, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 32, scope: random() },
                        nlas: vec![
                            netlink_packet_route::address::nlas::Nla::Address(vec![2, 3, 4, 5]),
                        ]
                    })) };
                    let address10 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
                        header: AddressHeader { index: 110, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 32, scope: random() },
                        nlas: vec![
                            netlink_packet_route::address::nlas::Nla::Address(vec![3, 4, 5, 6]),
                        ]
                    })) };

                    Ok(vec![address7, address8, address10])
                },
                _ => Ok(vec![]),
            }
        });
        mock_netlink_socket.expect_send_message().withf(|message| {
            match message.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => true,
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => true,
                _ => false,
            }
        }).returning(|input| {
            match input.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetLink(_)) => {
                    let netlink_header = NetlinkHeader { sequence_number: input.header.sequence_number, flags: random(), port_number: random(), length: random(), message_type: random() };

                    let iface2 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 101, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice2")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface5 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 105, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice5")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Down),
                        ]
                    })) };
                    let iface7 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 107, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice7")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface8 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 108, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice8")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };
                    let iface9 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
                        header: LinkHeader { index: 109, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
                        nlas: vec![
                            netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice9")),
                            netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                        ]
                    })) };

                    Ok(vec![iface2, iface5, iface7, iface8, iface9])
                },
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::GetAddress(_)) => {
                    let netlink_header = NetlinkHeader { sequence_number: input.header.sequence_number, flags: random(), port_number: random(), length: random(), message_type: random() };

                    let address7 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
                        header: AddressHeader { index: 107, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 20, scope: random() },
                        nlas: vec![
                            netlink_packet_route::address::nlas::Nla::Address(vec![1, 2, 3, 4]),
                        ]
                    })) };
                    let address8 = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
                        header: AddressHeader { index: 108, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 32, scope: random() },
                        nlas: vec![
                            netlink_packet_route::address::nlas::Nla::Address(vec![2, 3, 4, 5]),
                        ]
                    })) };

                    Ok(vec![address7, address8])
                },
                _ => Ok(vec![]),
            }
        });

        let running = Arc::new(AtomicBool::new(false));
        let database = Arc::new(RwLock::new(InterfaceManagerDatabase::new()));
        let subject = InterfaceManagerWorker { config, running: running.clone(), database: database.clone(), netlink_socket: Arc::new(mock_netlink_socket), netlink_message_processor };

        subject.poll().await;
        database.write().await.set_link_status_item(NetworkStatusItem::new_with_refresh(
            CanonicalNetworkId::new(Some(109), Some(String::from("SomeLink9")), Some(String::from("SomeDevice9"))),
            NetworkLinkStatus::new(Some(String::from("SomeLink9")), String::from("SomeDevice9"), NetworkLinkOperationalState::Down),
            Instant::now().sub(Duration::from_secs(11)),
        ));
        database.write().await.set_interface_status_item(NetworkStatusItem::new_with_refresh(
            CanonicalNetworkId::new(Some(109), Some(String::from("SomeInterface9")), Some(String::from("SomeDevice9"))),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface9")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink9")), String::from("SomeDevice9"), NetworkLinkOperationalState::Down)),
            Instant::now().sub(Duration::from_secs(11))
        ));

        subject.poll().await;
        running.store(false, Ordering::SeqCst);

        assert_eq!(database.read().await.list_link_status(), vec![
            NetworkLinkStatus::new(Some(String::from("SomeLink2")), String::from("SomeDevice2"), NetworkLinkOperationalState::Down),
            NetworkLinkStatus::new(Some(String::from("SomeLink3")), String::from("SomeDevice3"), NetworkLinkOperationalState::NotFound),
            NetworkLinkStatus::new(Some(String::from("SomeLink5")), String::from("SomeDevice5"), NetworkLinkOperationalState::Down),
            NetworkLinkStatus::new(Some(String::from("SomeLink6")), String::from("SomeDevice6"), NetworkLinkOperationalState::NotFound),
            NetworkLinkStatus::new(Some(String::from("SomeLink7")), String::from("SomeDevice7"), NetworkLinkOperationalState::Up),
            NetworkLinkStatus::new(Some(String::from("SomeLink8")), String::from("SomeDevice8"), NetworkLinkOperationalState::Up),
            NetworkLinkStatus::new(Some(String::from("SomeLink9")), String::from("SomeDevice9"), NetworkLinkOperationalState::Up),
        ]);
        assert_eq!(database.read().await.list_interface_status(), vec![
            NetworkInterfaceStatus::new(None, vec![IpAddress::new(IpAddr::from_str("2.3.4.5")?, 32)], NetworkLinkStatus::new(Some(String::from("SomeLink8")), String::from("SomeDevice8"), NetworkLinkOperationalState::Up)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface4")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink4")), String::from(""), NetworkLinkOperationalState::Misconfigured)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface5")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink5")), String::from("SomeDevice5"), NetworkLinkOperationalState::Down)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface6")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink6")), String::from("SomeDevice6"), NetworkLinkOperationalState::NotFound)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface7")), vec![IpAddress::new(IpAddr::from_str("1.2.3.4")?, 20)], NetworkLinkStatus::new(Some(String::from("SomeLink7")), String::from("SomeDevice7"), NetworkLinkOperationalState::Up)),
            NetworkInterfaceStatus::new(Some(String::from("SomeInterface9")), vec![], NetworkLinkStatus::new(Some(String::from("SomeLink9")), String::from("SomeDevice9"), NetworkLinkOperationalState::Up)),
        ]);

        Ok(())
    }

    #[tokio::test]
    pub async fn test_interface_listener() -> Result<(), Box<dyn Error + Send + Sync>> {
        let config = Arc::new(generate_test_config());
        let netlink_header = NetlinkHeader { sequence_number: random(), flags: random(), port_number: random(), length: random(), message_type: random() };
        let netlink_message = NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewLink(LinkMessage {
            header: LinkHeader { index: 101, link_layer_type: random(), change_mask: random(), flags: random(), interface_family: random() },
            nlas: vec![
                netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice2")),
                netlink_packet_route::link::nlas::Nla::OperState(State::Up),
            ]
        })) };

        let database = Arc::new(RwLock::new(InterfaceManagerDatabase::new()));
        database.write().await.set_link_status_item(NetworkStatusItem::new(CanonicalNetworkId::new(Some(110), None, Some(String::from("SomeDeviceA"))), NetworkLinkStatus::new(None, String::from("SomeDeviceA"), NetworkLinkOperationalState::Down)));
        println!("{:?}", database.read().await.list_interface_status());

        let subject = InterfaceManagerNetlinkSocketListener::new(Arc::new(NetlinkMessageProcessor::new(config)), database);
        subject.message_received(netlink_message).await;

        Ok(())
    }
    
    #[tokio::test]
    pub async fn test_interface_manager_list_empty() -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut mock_netlink_socket = MockNetlinkSocket::new();
        let mut mock_netlink_socket_factory = MockNetlinkSocketFactory::new();

        expect_get_link(&mut mock_netlink_socket, vec![]);
        expect_get_address(&mut mock_netlink_socket, vec![]);

        let mock_netlink_socket = Arc::new(mock_netlink_socket);
        mock_netlink_socket_factory.expect_create_socket().returning(move |_| {
            Ok(mock_netlink_socket.clone())
        });
        
        let mock_netlink_socket_factory = Arc::new(mock_netlink_socket_factory);
        let subject = InterfaceManager::new(Arc::new(rusty_router_model::Router::new(HashMap::new(), HashMap::new(), HashMap::new())), mock_netlink_socket_factory).await?;

        assert!(subject.list_network_links().await == vec![]);
        assert!(subject.list_network_interfaces().await == vec![]);

        Ok(())
    }

    fn generate_test_config() -> Router {
        Router::new(vec![
            (String::from("SomeLink2"), NetworkLink::new(String::from("SomeDevice2"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink3"), NetworkLink::new(String::from("SomeDevice3"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink5"), NetworkLink::new(String::from("SomeDevice5"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink6"), NetworkLink::new(String::from("SomeDevice6"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink7"), NetworkLink::new(String::from("SomeDevice7"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink8"), NetworkLink::new(String::from("SomeDevice8"), NetworkLinkType::GenericInterface)),
            (String::from("SomeLink9"), NetworkLink::new(String::from("SomeDevice9"), NetworkLinkType::GenericInterface)),
        ].drain(..).collect(), vec![
            (String::from("SomeInterface4"), NetworkInterface::new(None, String::from("SomeLink4"), vec![])),
            (String::from("SomeInterface5"), NetworkInterface::new(None, String::from("SomeLink5"), vec![])),
            (String::from("SomeInterface6"), NetworkInterface::new(None, String::from("SomeLink6"), vec![])),
            (String::from("SomeInterface7"), NetworkInterface::new(None, String::from("SomeLink7"), vec![])),
            (String::from("SomeInterface9"), NetworkInterface::new(None, String::from("SomeLink9"), vec![])),
        ].drain(..).collect(), HashMap::new())
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
