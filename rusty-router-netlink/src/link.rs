use std::error::Error;
use std::collections::HashMap;

use log::warn;

use netlink_packet_core;
use netlink_packet_route;
use netlink_packet_route::rtnl::link::nlas;

use rusty_router_model;

use crate::packet;
use crate::socket::NetlinkSocket;

pub struct NetlinkRustyRouterLink {}
struct NetlinkRustyRouterLinkResult (u64, rusty_router_model::NetworkInterface);

impl NetlinkRustyRouterLink {
    pub fn new() -> NetlinkRustyRouterLink {
        NetlinkRustyRouterLink {}
    }

    pub fn list_network_interfaces(&self, socket: &Box<dyn NetlinkSocket>) -> Result<HashMap<u64, rusty_router_model::NetworkInterface>, Box<dyn Error>> {
        let link_message = netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = packet::build_default_packet(link_message);
        let messages = socket.send_message(packet)?;

        let mut result: HashMap<u64, rusty_router_model::NetworkInterface> = HashMap::new();
        for message in messages {
            self.process_link_message(message).into_iter().for_each(|data| { result.insert(data.0, data.1).iter().for_each(|old_value| warn!("Duplicate interface index: {:?}", old_value)); });
        }
        Ok(result)
    }

    fn process_link_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<NetlinkRustyRouterLinkResult> {
        let mut index: Option<u64> = None;
        let mut name: Option<String> = None;

        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) = message.payload {
            index = Some(msg.header.index as u64);
            for attribute in msg.nlas.iter() {
                if let nlas::Nla::IfName(ifname) = attribute {
                    name = Some(ifname.clone())
                }
            }
        } else {
            warn!("Netlink data does not contain a payload: {:?}", message)
        }

        index.and_then(|index| name.and_then(|name| Some(NetlinkRustyRouterLinkResult(index, rusty_router_model::NetworkInterface {
            device: name,
            network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
        }))))
    }
}