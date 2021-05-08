use std::error::Error;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use log::warn;

use netlink_packet_core;
use netlink_packet_route;
use netlink_packet_route::rtnl::address::nlas;

use rusty_router_model;

use crate::packet;
use crate::socket::NetlinkSocket;

pub struct NetlinkRustyRouterAddress {}
struct NetlinkRustyRouterAddressResult (u64, rusty_router_model::IpAddress);

impl NetlinkRustyRouterAddress {
    pub fn new() -> NetlinkRustyRouterAddress {
        NetlinkRustyRouterAddress {}
    }

    pub fn list_router_interfaces(&self, socket: &Box<dyn NetlinkSocket>, network_interfaces: &HashMap<u64, crate::link::NetlinkRustyRouterLinkStatus>) -> Result<HashMap<u64, rusty_router_model::RouterInterface>, Box<dyn Error>> {
        let link_message = netlink_packet_route::RtnlMessage::GetAddress(netlink_packet_route::AddressMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = packet::build_default_packet(link_message);
        let messages = socket.send_message(packet)?;

        let mut result: HashMap<u64, rusty_router_model::RouterInterface> = HashMap::new();
        for message in messages {
            self.process_address_message(message).into_iter().for_each(|iface| { match network_interfaces.get(&iface.0) {
                Some(network_interface) => {
                    result.entry(iface.0).or_insert(rusty_router_model::RouterInterface {
                        network_interface: network_interface.name.clone(),
                        ip_addresses: vec![],
                    });
                },
                // TODO This message sucks.  It give no actionable details.
                None => warn!("Physical interfaces were not found for routes"),
            }});
        }
        Ok(result)
    }

    // TODO Decode addresses
    fn process_address_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<NetlinkRustyRouterAddressResult> {
        let mut index: Option<u64> = None;
        let mut _prefix: Option<u64> = None;
        let mut address: Option<String> = None;
        let mut _address6: Option<String> = None;

        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewAddress(msg)) = message.payload {
            index = Some(msg.header.index as u64);
            _prefix = Some(msg.header.prefix_len as u64);

            if msg.header.family as u16 == netlink_packet_route::AF_INET {
                for attribute in msg.nlas.iter() {
                    if let nlas::Nla::Address(data) = attribute {
                        if data.len() == 4 {
                            address = Some(Ipv4Addr::from([data[0], data[1], data[2], data[3]]).to_string())
                        }
                    }
                }
            }
            if msg.header.family as u16 == netlink_packet_route::AF_INET6 {
                for attribute in msg.nlas.iter() {
                    if let nlas::Nla::Address(data) = attribute {
                        if data.len() == 16 {
                            address = Some(Ipv6Addr::from([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]).to_string())
                        }
                    }
                }
            }

            println!("{:?} {:?}/{:?}", address, _address6, _prefix);
        } else {
            warn!("Netlink data does not contain a payload: {:?}", message)
        }

        index.and_then(|index| Some(NetlinkRustyRouterAddressResult(index, rusty_router_model::IpAddress (
            rusty_router_model::IpAddressType::IpV4, "TODO".to_string(), 32
        ))))
    }
}