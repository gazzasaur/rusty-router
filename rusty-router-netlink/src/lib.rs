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

pub struct Netlink {
    socket: netlink_sys::Socket,
}

impl Netlink {
    pub fn new() -> Result<Netlink, Box<dyn Error>> {
        let mut socket = netlink_sys::Socket::new(protocols::NETLINK_ROUTE)?;
        socket.bind_auto()?;
        socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;
        Ok(Netlink {
            socket
        })
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
    
        let mut buf = vec![0; packet.header.length as usize];
        packet.serialize(&mut buf[..]);
        self.socket.send(&buf[..], 0).unwrap();
    
        let mut peer_interfaces: HashMap<u32, u32> = HashMap::new();
        let mut interface_by_name: HashMap<String, u32> = HashMap::new();
        let mut interface_by_index: HashMap<u32, String> = HashMap::new();

        let mut result: Vec<rusty_router_model::Interface> = Vec::new();
        let mut receive_buffer = vec![0; 4096];
        let mut offset = 0;

        loop {
            let size = self.socket.recv(&mut receive_buffer[..], 0)?;
    
            loop {
                let bytes = &receive_buffer[offset..];
                let rx_packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = netlink_packet_route::NetlinkMessage::deserialize(bytes)?;
    
                if rx_packet.payload == netlink_packet_core::NetlinkPayload::Done {
                    for interface in result.iter_mut().filter(|dev| dev.interface_type == rusty_router_model::InterfaceType::VirtualEthernetUnassigned) {
                        interface_by_name.get(&interface.device).and_then(|id| peer_interfaces.get(&id)).and_then(|id| interface_by_index.get(&id)).iter().for_each(|&peer_name| {
                            interface.interface_type = rusty_router_model::InterfaceType::VirtualEthernet(peer_name.clone())
                        });
                    };
                    return Ok(result);
                }

                let mut index: u32 = 0;
                let mut link: Option<u32> = None;
                let mut name: Option<String> = None;
                let mut info: Option<Vec<nlas::Info>> = None;
                let mut _operationally_enabled: Option<bool> = None;

                match rx_packet.payload {
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
                        warn!("Netlink data does not contain a payload: {:?}", rx_packet)
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

                offset += rx_packet.header.length as usize;
                if offset == size || rx_packet.header.length == 0 {
                    offset = 0;
                    break;
                }
            }
        }
    }

    fn create_interface(&self, _interface: rusty_router_model::Interface) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
