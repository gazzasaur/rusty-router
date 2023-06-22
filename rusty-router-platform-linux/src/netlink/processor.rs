use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use log::warn;
use netlink_packet_route::link::nlas;
use rusty_router_model::{NetworkLinkStatus, Router};

pub struct NetlinkMessageProcessor {
    device_links: HashMap<String, String>,
}
impl NetlinkMessageProcessor {
    pub fn new(config: Arc<Router>) -> NetlinkMessageProcessor {
        let device_links = config
            .get_network_links()
            .iter()
            .map(|(name, link)| (link.get_device().clone(), name.clone()))
            .collect();
        NetlinkMessageProcessor { device_links }
    }

    pub fn process_link_message(
        &self,
        message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>,
    ) -> Option<(u64, NetworkLinkStatus)> {
        let mut device: Option<String> = None;
        let mut state = rusty_router_model::NetworkLinkOperationalState::Unknown;

        let msg = match message.payload {
            netlink_packet_core::NetlinkPayload::InnerMessage(
                netlink_packet_route::RtnlMessage::NewLink(msg),
            ) => msg,
            netlink_packet_core::NetlinkPayload::InnerMessage(
                netlink_packet_route::RtnlMessage::DelLink(msg),
            ) => msg,
            _ => {
                warn!("Netlink data does not contain a payload: {:?}", message);
                return None;
            }
        };

        let index = msg.header.index as u64;
        for attribute in msg.nlas.iter() {
            if let nlas::Nla::IfName(ifname) = attribute {
                device = Some(ifname.clone())
            } else if let nlas::Nla::OperState(operational_state) = attribute {
                state = match operational_state {
                    nlas::State::Up => rusty_router_model::NetworkLinkOperationalState::Up,
                    nlas::State::Down => rusty_router_model::NetworkLinkOperationalState::Down,
                    _ => rusty_router_model::NetworkLinkOperationalState::Unknown,
                }
            }
        }
        device.and_then(|device| {
            Some((
                index,
                NetworkLinkStatus::new(
                    self.device_links.get(&device).map(|x| x.clone()),
                    device,
                    state,
                ),
            ))
        })
    }

    pub fn process_address_message(
        &self,
        message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>,
    ) -> Option<(u64, rusty_router_model::IpAddress)> {
        let mut address: Option<IpAddr> = None;

        let msg = match message.payload {
            netlink_packet_core::NetlinkPayload::InnerMessage(
                netlink_packet_route::RtnlMessage::NewAddress(msg),
            ) => msg,
            netlink_packet_core::NetlinkPayload::InnerMessage(
                netlink_packet_route::RtnlMessage::DelAddress(msg),
            ) => msg,
            _ => {
                warn!("Netlink data does not contain a payload: {:?}", message);
                return None;
            }
        };

        let index = msg.header.index as u64;
        // Here we trust the prefix from the OS.  Validation that is too strict here could lead to compatibility issues.
        let prefix = msg.header.prefix_len as u64;

        if msg.header.family as u16 == netlink_packet_route::AF_INET {
            for attribute in msg.nlas.iter() {
                if let netlink_packet_route::address::nlas::Nla::Address(data) = attribute {
                    if data.len() == 4 {
                        address = Some(IpAddr::V4(Ipv4Addr::from([
                            data[0], data[1], data[2], data[3],
                        ])));
                    }
                }
            }
        }
        if msg.header.family as u16 == netlink_packet_route::AF_INET6 {
            for attribute in msg.nlas.iter() {
                if let netlink_packet_route::address::nlas::Nla::Address(data) = attribute {
                    if data.len() == 16 {
                        address = Some(IpAddr::V6(Ipv6Addr::from([
                            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                            data[8], data[9], data[10], data[11], data[12], data[13], data[14],
                            data[15],
                        ])))
                    }
                }
            }
        }

        address.and_then(|address| Some((index, rusty_router_model::IpAddress(address, prefix))))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, error::Error, sync::Arc};

    use crate::netlink::NetlinkMessageProcessor;
    use netlink_packet_core::{NetlinkHeader, NetlinkMessage, NetlinkPayload};
    use netlink_packet_route::{nlas::link::State, LinkHeader, LinkMessage, RtnlMessage};
    use rusty_router_model::{NetworkLink, NetworkLinkStatus, NetworkLinkType, Router};

    #[tokio::test]
    pub async fn test_process_link_message() -> Result<(), Box<dyn Error + Send + Sync>> {
        let netlink_header = NetlinkHeader::default();

        let config = Arc::new(Router::new(HashMap::new(), HashMap::new(), HashMap::new()));
        let subject = NetlinkMessageProcessor::new(config);

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::NewLink({
                    let mut link_message = LinkMessage::default();
                    link_message.nlas = vec![];
                    link_message
                }))
            )) == None
        );

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::NewLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = {
                        let mut link_header = LinkHeader::default();
                        link_header.index = 10;
                        link_header
                    };
                    link_message.nlas = vec![netlink_packet_route::link::nlas::Nla::IfName(
                        String::from("SomeDevice"),
                    )];
                    link_message
                }))
            )) == Some((
                10,
                NetworkLinkStatus::new(
                    None,
                    String::from("SomeDevice"),
                    rusty_router_model::NetworkLinkOperationalState::Unknown
                )
            ))
        );

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::NewLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = {
                        let mut link_header = LinkHeader::default();
                        link_header.index = 10;
                        link_header
                    };
                    link_message.nlas = vec![netlink_packet_route::link::nlas::Nla::IfName(
                        String::from("SomeDevice"),
                    )];
                    link_message
                }))
            )) == Some((
                10,
                NetworkLinkStatus::new(
                    None,
                    String::from("SomeDevice"),
                    rusty_router_model::NetworkLinkOperationalState::Unknown
                )
            ))
        );

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::NewLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = {
                        let mut link_header = LinkHeader::default();
                        link_header.index = 15;
                        link_header
                    };
                    link_message.nlas = vec![
                        netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")),
                        netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                    ];
                    link_message
                }))
            )) == Some((
                15,
                NetworkLinkStatus::new(
                    None,
                    String::from("SomeDevice1"),
                    rusty_router_model::NetworkLinkOperationalState::Up
                )
            ))
        );

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::DelLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = {
                        let mut link_header = LinkHeader::default();
                        link_header.index = 15;
                        link_header
                    };
                    link_message.nlas = vec![
                        netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")),
                        netlink_packet_route::link::nlas::Nla::OperState(State::Other(100)),
                    ];
                    link_message
                }))
            )) == Some((
                15,
                NetworkLinkStatus::new(
                    None,
                    String::from("SomeDevice1"),
                    rusty_router_model::NetworkLinkOperationalState::Unknown
                )
            ))
        );

        // This one logs an error message and continues.  There is nothing a user can really do here.  It really should not be possible to reach.
        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::GetLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = {
                        let mut link_header = LinkHeader::default();
                        link_header.index = 15;
                        link_header
                    };
                    link_message.nlas = vec![
                        netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")),
                        netlink_packet_route::link::nlas::Nla::OperState(State::Other(100)),
                    ];
                    link_message
                }))
            )) == None
        );

        let config = Arc::new(Router::new(
            vec![
                (
                    String::from("NetworkLink1"),
                    NetworkLink::new(String::from("Device1"), NetworkLinkType::GenericInterface),
                ),
                (
                    String::from("NetworkLink2"),
                    NetworkLink::new(String::from("Device2"), NetworkLinkType::GenericInterface),
                ),
                (
                    String::from("NetworkLink3"),
                    NetworkLink::new(String::from("Device3"), NetworkLinkType::GenericInterface),
                ),
            ]
            .drain(..)
            .collect(),
            HashMap::new(),
            HashMap::new(),
        ));
        let subject = NetlinkMessageProcessor::new(config);

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::NewLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = LinkHeader::default();
                    link_message.nlas = vec![];
                    link_message
                }))
            )) == None
        );

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::NewLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = {
                        let mut link_header = LinkHeader::default();
                        link_header.index = 15;
                        link_header
                    };
                    link_message.nlas = vec![
                        netlink_packet_route::link::nlas::Nla::IfName(String::from("SomeDevice1")),
                        netlink_packet_route::link::nlas::Nla::OperState(State::Up),
                    ];
                    link_message
                }))
            )) == Some((
                15,
                NetworkLinkStatus::new(
                    None,
                    String::from("SomeDevice1"),
                    rusty_router_model::NetworkLinkOperationalState::Up
                )
            ))
        );

        assert!(
            subject.process_link_message(NetlinkMessage::new(
                netlink_header,
                NetlinkPayload::InnerMessage(RtnlMessage::NewLink({
                    let mut link_message = LinkMessage::default();
                    link_message.header = {
                        let mut link_header = LinkHeader::default();
                        link_header.index = 20;
                        link_header
                    };
                    link_message.nlas = vec![
                        netlink_packet_route::link::nlas::Nla::IfName(String::from("Device2")),
                        netlink_packet_route::link::nlas::Nla::OperState(State::Down),
                    ];
                    link_message
                }))
            )) == Some((
                20,
                NetworkLinkStatus::new(
                    Some(String::from("NetworkLink2")),
                    String::from("Device2"),
                    rusty_router_model::NetworkLinkOperationalState::Down
                )
            ))
        );

        Ok(())
    }

    #[tokio::test]
    pub async fn test_process_address_message() -> Result<(), Box<dyn Error + Send + Sync>> {
        // let netlink_header = NetlinkHeader { sequence_number: random(), flags: random(), port_number: random(), length: random(), message_type: random() };

        // let config = Arc::new(Router::new(HashMap::new(), HashMap::new(), HashMap::new()));
        // let subject = NetlinkMessageProcessor::new(config);

        // assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
        //     header: AddressHeader { index: random(), flags: random(), family: random(), prefix_len: random(), scope: random() },
        //     nlas: vec![]
        // })) }) == None);

        // assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::NewAddress(AddressMessage {
        //     header: AddressHeader { index: 10, flags: random(), family: netlink_packet_route::AF_INET as u8, prefix_len: 20, scope: random() },
        //     nlas: vec![netlink_packet_route::address::nlas::Nla::Address(vec![1, 2, 3, 4])]
        // })) }) == Some((10, IpAddress::new(IpAddr::from_str("1.2.3.4")?, 20))));

        // assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::DelAddress(AddressMessage {
        //     header: AddressHeader { index: 15, flags: random(), family: netlink_packet_route::AF_INET6 as u8, prefix_len: 56, scope: random() },
        //     nlas: vec![netlink_packet_route::address::nlas::Nla::Address(vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1])]
        // })) }) == Some((15, IpAddress::new(IpAddr::from_str("100f:0e0d:0c0b:0a09:0807:0605:0403:0201")?, 56))));

        // assert!(subject.process_address_message(NetlinkMessage { header: netlink_header, payload: NetlinkPayload::InnerMessage(RtnlMessage::GetAddress(AddressMessage {
        //     header: AddressHeader { index: 15, flags: random(), family: netlink_packet_route::AF_INET6 as u8, prefix_len: 56, scope: random() },
        //     nlas: vec![netlink_packet_route::address::nlas::Nla::Address(vec![16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1])]
        // })) }) == None);

        Ok(())
    }
}
