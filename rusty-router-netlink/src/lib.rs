use mockall::*;
use mockall::predicate::*;

use std::error::Error;
use std::collections::HashMap;

use log::warn;

use netlink_sys;
use netlink_packet_core;
use netlink_packet_route;
use netlink_sys::protocols;
use netlink_packet_route::constants;
use netlink_packet_route::rtnl::link::nlas;

use rusty_router_model;
use rusty_router_model::DeviceInterface;

#[automock]
pub trait NetlinkSocket {
    fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>>;
}

pub struct DefaultNetlinkSocket {
    socket: netlink_sys::Socket,
}

impl DefaultNetlinkSocket {
    pub fn new() -> Result<DefaultNetlinkSocket, Box<dyn Error>> {
        let mut socket = netlink_sys::Socket::new(protocols::NETLINK_ROUTE)?;
        socket.bind_auto()?;
        socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;
        Ok(DefaultNetlinkSocket { socket })
    }
}

impl NetlinkSocket for DefaultNetlinkSocket {
    fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>> {
        let mut buf = vec![0; message.header.length as usize];
        message.serialize(&mut buf[..]);
        self.socket.send(&buf[..], 0)?;

        let mut offset = 0;
        let mut receive_buffer = vec![0; 4096];
        let mut received_messages = Vec::new();

        loop {
            let size = self.socket.recv(&mut receive_buffer[..], 0)?;

            loop {
                let bytes = &receive_buffer[offset..];
                let rx_packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = netlink_packet_route::NetlinkMessage::deserialize(bytes)?;
                let header_length = rx_packet.header.length as usize;

                if rx_packet.payload == netlink_packet_core::NetlinkPayload::Done {
                    return Ok(received_messages);
                }

                received_messages.push(rx_packet);

                offset += header_length;
                if offset == size || header_length == 0 {
                    offset = 0;
                    break;
                }
            }
        }
    }
}

pub struct Netlink {
    netlink_socket: Box<dyn NetlinkSocket>,
}

impl Netlink {
    pub fn new(netlink_socket: Box<dyn NetlinkSocket>) -> Netlink {
        Netlink { netlink_socket }
    }
}

impl DeviceInterface for Netlink {
    fn list_interfaces(&self) -> Result<Vec<rusty_router_model::Interface>, Box<dyn Error>> {
        let mut packet = netlink_packet_core::NetlinkMessage {
            header: netlink_packet_core::NetlinkHeader::default(),
            payload: netlink_packet_core::NetlinkPayload::from(netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default())),
        };
        packet.header.flags = constants::NLM_F_DUMP | constants::NLM_F_REQUEST;
        packet.header.sequence_number = 1;
        packet.finalize();

        let messages = self.netlink_socket.send_message(packet)?;
    
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_no_interfaces() {
        let mut mock = MockNetlinkSocket::new();
        mock.expect_send_message().returning(|_| {
            Ok(vec![])
        });
        assert!(Netlink::new(Box::new(mock)).list_interfaces().unwrap().len() == 0);
    }
}
