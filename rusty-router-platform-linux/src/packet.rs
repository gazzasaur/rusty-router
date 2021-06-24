use rand::Rng;

use netlink_packet_core;
use netlink_packet_route;
use netlink_packet_route::constants;

pub fn build_default_packet(message: netlink_packet_route::RtnlMessage) -> netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> {
    let mut packet = netlink_packet_core::NetlinkMessage {
        header: netlink_packet_core::NetlinkHeader::default(),
        payload: netlink_packet_core::NetlinkPayload::from(message),
    };
    packet.header.flags = constants::NLM_F_DUMP | constants::NLM_F_REQUEST;
    packet.header.sequence_number = rand::thread_rng().gen();
    packet.finalize();

    return packet;
}
