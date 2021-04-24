use std::error::Error;

use log::warn;

use netlink_packet_core;
use netlink_packet_route;
use netlink_packet_route::rtnl::link::nlas;

use rusty_router_model;

use crate::packet;
use crate::socket::NetlinkSocket;

pub struct NetlinkRustyRouterAddress {}

impl NetlinkRustyRouterAddress {
    pub fn new() -> NetlinkRustyRouterAddress {
        NetlinkRustyRouterAddress {}
    }

    pub fn list_router_interfaces(&self, socket: &Box<dyn NetlinkSocket>) -> Result<Vec<rusty_router_model::RouterInterface>, Box<dyn Error>> {
        let link_message = netlink_packet_route::RtnlMessage::GetAddress(netlink_packet_route::AddressMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = packet::build_default_packet(link_message);
        let messages = socket.send_message(packet)?;

        println!("{:?}", messages);

        let mut result: Vec<rusty_router_model::RouterInterface> = Vec::new();
        for message in messages {
            self.process_address_message(message).into_iter().for_each(|iface| result.push(iface));
        }
        Ok(result)
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

        // let iface_parts = name.and_then(|n| {
        //     let split_name = n.split(":");
        //     Some(split_name.next())
        // });
        // if let Some(ifname) = name.and_then( {


        //     return Some(rusty_router_model::RouterInterface {
                
        //     });
        // }
        None
    }

    // fn parse_interface(&self, raw_address_interface) -> Option<rusty_router_model::RouterInterface> 
}