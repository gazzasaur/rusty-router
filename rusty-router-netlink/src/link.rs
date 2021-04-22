use std::error::Error;
use std::collections::HashMap;

use log::warn;

use netlink_packet_core;
use netlink_packet_route;
use netlink_packet_route::constants;
use netlink_packet_route::rtnl::link::nlas;

use rusty_router_model;

use crate::socket::NetlinkSocket;

pub struct NetlinkRustyRouterLink {}

impl NetlinkRustyRouterLink {
    pub fn new() -> NetlinkRustyRouterLink {
        NetlinkRustyRouterLink {}
    }

    pub fn list_interfaces(&self, socket: &Box<dyn NetlinkSocket>) -> Result<Vec<rusty_router_model::Interface>, Box<dyn Error>> {
        let mut packet = netlink_packet_core::NetlinkMessage {
            header: netlink_packet_core::NetlinkHeader::default(),
            payload: netlink_packet_core::NetlinkPayload::from(netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default())),
        };
        packet.header.flags = constants::NLM_F_DUMP | constants::NLM_F_REQUEST;
        packet.header.sequence_number = 1;
        packet.finalize();

        let messages = socket.send_message(packet)?;
    
        let mut peer_interfaces: HashMap<u32, u32> = HashMap::new();
        let mut interface_by_name: HashMap<String, u32> = HashMap::new();
        let mut interface_by_index: HashMap<u32, String> = HashMap::new();

        let mut result: Vec<rusty_router_model::Interface> = Vec::new();

        for message in messages {
            let mut index: u32 = 0;
            let mut link: Option<u32> = None;
            let mut name: Option<String> = None;
            let mut info: Option<Vec<nlas::Info>> = None;
            let mut _operationally_enabled: Option<bool> = None;

            match message.payload {
                netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) => {
                    index = msg.header.index;
                    for i in msg.nlas.iter() {
                        match i {
                            nlas::Nla::IfName(n) => name = Some(n.clone()),
                            nlas::Nla::Info(n) => info = Some(n.clone()),
                            nlas::Nla::Link(n) => link = Some(*n),
                            _ => (),
                        }
                    }
                },
                _ => {
                    warn!("Netlink data does not contain a payload: {:?}", message)
                }
            }

            let interface_type: Option<rusty_router_model::InterfaceType> = info.and_then(|info_items| info_items.iter().find_map(|info_item| match info_item {
                nlas::Info::Kind(nlas::InfoKind::Veth) => Some(rusty_router_model::InterfaceType::VirtualEthernetUnassigned),
                _ => None,
            }));
            let interface_type = interface_type.unwrap_or(rusty_router_model::InterfaceType::Ethernet);

            name.and_then(|name| {
                interface_by_name.insert(name.clone(), index);
                interface_by_index.insert(index, name.clone());
                Some(result.push(rusty_router_model::Interface {
                    device: name,
                    interface_type,
                }))
            });
            link.iter().for_each(|&peer_index| { peer_interfaces.insert(index, peer_index); () });
        }
        Ok(result)
    }
}