use std::error::Error;

use log::warn;

use netlink_packet_core;
use netlink_packet_route;
use netlink_packet_route::constants;
use netlink_packet_route::rtnl::link::nlas;

use rusty_router_model;

use crate::socket::NetlinkSocket;

pub struct NetlinkRustyRouterAddress {}

impl NetlinkRustyRouterAddress {
    pub fn new() -> NetlinkRustyRouterAddress {
        NetlinkRustyRouterAddress {}
    }

    pub fn list_router_interfaces(&self, socket: &Box<dyn NetlinkSocket>) -> Result<Vec<rusty_router_model::RouterInterface>, Box<dyn Error>> {
        let link_message = netlink_packet_route::RtnlMessage::GetAddress(netlink_packet_route::AddressMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = self.build_default_packet(link_message);
        let messages = socket.send_message(packet)?;

        let mut result: Vec<rusty_router_model::RouterInterface> = Vec::new();
        for message in messages {
            self.process_address_message(message).into_iter().for_each(|iface| result.push(iface));
        }
        Ok(result)
    }

    fn build_default_packet(&self, message: netlink_packet_route::RtnlMessage) -> netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> {
        let mut packet = netlink_packet_core::NetlinkMessage {
            header: netlink_packet_core::NetlinkHeader::default(),
            payload: netlink_packet_core::NetlinkPayload::from(message),
        };
        packet.header.flags = constants::NLM_F_DUMP | constants::NLM_F_REQUEST;
        packet.header.sequence_number = 1;
        packet.finalize();

        return packet;
    }

    fn process_address_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<rusty_router_model::RouterInterface> {
        let mut _name: Option<String> = None;

        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) = message.payload {
            for attribute in msg.nlas.iter() {
                if let nlas::Nla::IfName(ifname) = attribute {
                    _name = Some(ifname.clone())
                }
            }
        } else {
            warn!("Netlink data does not contain a payload: {:?}", message)
        }

        // if let Some(ifname) = name {
        //     return Some(rusty_router_model::RouterInterface {
                
        //     });
        // }
        None
    }
}