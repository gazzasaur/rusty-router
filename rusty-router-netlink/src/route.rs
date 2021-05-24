use std::sync::Arc;
use std::error::Error;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use log::warn;

use netlink_packet_route::rtnl::address::nlas;

use rusty_router_model;

use crate::packet;
use crate::socket::NetlinkSocket;

pub struct NetlinkRustyRouterAddress {}

#[derive(Debug)]
pub struct NetlinkRustyRouterAddressResult {
    pub index: u64,
    pub address: rusty_router_model::IpAddress
}

#[derive(Debug)]
pub struct NetlinkRustyRouterDeviceAddressesResult {
    pub addresses: Vec<rusty_router_model::IpAddress>
}
impl NetlinkRustyRouterAddress {
    pub fn new() -> NetlinkRustyRouterAddress {
        NetlinkRustyRouterAddress {}
    }

    pub async fn list_router_interfaces(&self, socket: &Arc<dyn NetlinkSocket + Send + Sync>) -> Result<HashMap<u64, NetlinkRustyRouterDeviceAddressesResult>, Box<dyn Error>> {
        let link_message = netlink_packet_route::RtnlMessage::GetAddress(netlink_packet_route::AddressMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = packet::build_default_packet(link_message);
        let messages = socket.send_message(packet).await?;

        let mut result: HashMap<u64, NetlinkRustyRouterDeviceAddressesResult> = HashMap::new();
        for message in messages {
            self.process_address_message(message).into_iter().for_each(|iface| {
                result.entry(iface.index).or_insert(NetlinkRustyRouterDeviceAddressesResult {
                    addresses: vec![]
                }).addresses.push(iface.address);
            })
        }
        Ok(result)
    }

    fn process_address_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<NetlinkRustyRouterAddressResult> {
        let mut index: Option<u64> = None;
        let mut prefix: Option<u64> = None;
        let mut address: Option<String> = None;
        let mut family: Option<rusty_router_model::IpAddressType> = None;

        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewAddress(msg)) = message.payload {
            index = Some(msg.header.index as u64);
            prefix = Some(msg.header.prefix_len as u64);

            if msg.header.family as u16 == netlink_packet_route::AF_INET {
                for attribute in msg.nlas.iter() {
                    if let nlas::Nla::Address(data) = attribute {
                        if data.len() == 4 {
                            family = Some(rusty_router_model::IpAddressType::IpV4);
                            address = Some(Ipv4Addr::from([data[0], data[1], data[2], data[3]]).to_string());
                        }
                    }
                }
            }
            if msg.header.family as u16 == netlink_packet_route::AF_INET6 {
                for attribute in msg.nlas.iter() {
                    if let nlas::Nla::Address(data) = attribute {
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
}