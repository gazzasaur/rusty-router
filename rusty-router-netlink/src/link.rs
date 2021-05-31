use std::sync::Arc;
use std::error::Error;
use std::collections::HashMap;

use log::warn;

use netlink_packet_core;
use netlink_packet_route;
use netlink_packet_route::rtnl::link::nlas;

use crate::packet;
use crate::socket::NetlinkSocket;

#[derive(Debug, PartialEq, Eq)]
pub struct NetlinkRustyRouterLinkStatus {
    pub index: u64,
    pub name: String,
    pub state: rusty_router_model::NetworkInterfaceOperationalState,
}

pub struct NetlinkRustyRouterLink {
}
impl NetlinkRustyRouterLink {
    pub async fn list_network_interfaces(socket: &Arc<dyn NetlinkSocket + Send + Sync>) -> Result<HashMap<u64, NetlinkRustyRouterLinkStatus>, Box<dyn Error>> {
        let link_message = netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = packet::build_default_packet(link_message);

        let messages = socket.send_message(packet).await?;

        let mut result: HashMap<u64, NetlinkRustyRouterLinkStatus> = HashMap::new();
        for message in messages {
            NetlinkRustyRouterLink::process_link_message(message).into_iter().for_each(|data| { result.insert(data.index, data).iter().for_each(|old_value| warn!("Duplicate interface index: {:?}", old_value)); });
        }
        Ok(result)
    }

    fn process_link_message(message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Option<NetlinkRustyRouterLinkStatus> {
        let mut index: Option<u64> = None;
        let mut name: Option<String> = None;
        let mut state = rusty_router_model::NetworkInterfaceOperationalState::Unknown;

        if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) = message.payload {
            index = Some(msg.header.index as u64);
            for attribute in msg.nlas.iter() {
                if let nlas::Nla::IfName(ifname) = attribute {
                    name = Some(ifname.clone())
                } else if let nlas::Nla::OperState(operational_state) = attribute {
                    state = match operational_state {
                        nlas::State::Up => rusty_router_model::NetworkInterfaceOperationalState::Up,
                        nlas::State::Down => rusty_router_model::NetworkInterfaceOperationalState::Down,
                        _ => rusty_router_model::NetworkInterfaceOperationalState::Unknown,
                    }
                }
            }
        } else {
            warn!("Netlink data does not contain a payload: {:?}", message)
        }

        index.and_then(|index| name.and_then(|name| Some(NetlinkRustyRouterLinkStatus {
            index,
            name,
            state,
        })))
    }
}
