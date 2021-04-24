use std::error::Error;

use log::warn;

use netlink_packet_core;
use netlink_packet_route;
use std::sync::{Arc, RwLock};
use netlink_packet_route::rtnl::link::nlas;

use rusty_router_model;

use crate::cache;
use crate::packet;
use crate::socket::NetlinkSocket;

pub struct NetlinkRustyRouterLink {}

impl NetlinkRustyRouterLink {
    pub fn new() -> NetlinkRustyRouterLink {
        NetlinkRustyRouterLink {}
    }

    pub fn list_network_interfaces(&self, socket: &Box<dyn NetlinkSocket>, _cache: Arc<RwLock<cache::NetlinkRouterCache>>) -> Result<Vec<rusty_router_model::NetworkInterface>, Box<dyn Error>> {
        let link_message = netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = packet::build_default_packet(link_message);
        let messages = socket.send_message(packet)?;

        // let mut _m = rcache.write().map_err(|e| e.into());
//        rcache.write()?.update_interface_cache(32, "blah".to_string().clone());

        let mut result: Vec<rusty_router_model::NetworkInterface> = Vec::new();
        for message in messages {
            self.process_link_message(message).into_iter().for_each(|iface| result.push(iface));
        }
        Ok(result)
    }

    fn process_link_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<rusty_router_model::NetworkInterface> {
        let mut name: Option<String> = None;

        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) = message.payload {
            for attribute in msg.nlas.iter() {
                if let nlas::Nla::IfName(ifname) = attribute {
                    name = Some(ifname.clone())
                }
            }
        } else {
            warn!("Netlink data does not contain a payload: {:?}", message)
        }

        if let Some(ifname) = name {
            return Some(rusty_router_model::NetworkInterface {
                device: ifname,
                network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
            });
        }
        None
    }
}