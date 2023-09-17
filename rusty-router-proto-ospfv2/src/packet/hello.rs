use crate::packet::OspfAuthenticationType;
use std::{
    convert::{TryFrom, TryInto},
    net::Ipv4Addr,
};

use super::{header::{OspfHeader, PROTOCOL}, OspfHeaderBuilder, OSPF_V2_HEADER_LENGTH};
use rusty_router_proto_common::{error::ProtocolError, from_be_bytes, InternetChecksum};
use rusty_router_proto_ip::Ipv4Netmask;

#[derive(Clone, Debug)]
pub struct OspfHelloPacketBuilder {
    router_id: Ipv4Addr,
    area_id: Ipv4Addr,
    network_mask: Ipv4Netmask,
    hello_interval: u16,
    options: u8,
    router_priority: u8,
    router_dead_interval: u32,
    designated_router: Ipv4Addr,
    backup_designated_router: Ipv4Addr,
    neighbors: Vec<Ipv4Addr>,
}
impl OspfHelloPacketBuilder {
    pub fn new(
        router_id: Ipv4Addr,
        area_id: Ipv4Addr,
        network_mask: Ipv4Netmask,
        hello_interval: u16,
        options: u8,
        router_priority: u8,
        router_dead_interval: u32,
        designated_router: Ipv4Addr,
        backup_designated_router: Ipv4Addr,
        neighbors: Vec<Ipv4Addr>,
    ) -> Self {
        OspfHelloPacketBuilder {
            router_id,
            area_id,
            network_mask,
            hello_interval,
            options,
            router_priority,
            router_dead_interval,
            designated_router,
            backup_designated_router,
            neighbors,
        }
    }

    pub fn build(self) -> Result<OspfHelloPacket, ProtocolError> {
        if self.neighbors.len() > (65536 - (OSPF_V2_HEADER_LENGTH as usize) - 20)/4 - 1 {
            return Err(ProtocolError::UnsupportedFieldValue(PROTOCOL, "Neighbor List Length", self.neighbors.len()))
        }
        let packet_length = OSPF_V2_HEADER_LENGTH + 20 + 4*self.neighbors.len() as u16;

        let header_builder = OspfHeaderBuilder::new(
            super::OspfMessageType::Hello,
            packet_length,
            self.router_id,
            self.area_id,
        );

        let mut packet = OspfHelloPacket {
            header: header_builder.clone().build(),
            network_mask: self.network_mask,
            hello_interval: self.hello_interval,
            options: self.options,
            router_priority: self.router_priority,
            backup_designated_router: self.backup_designated_router,
            designated_router: self.designated_router,
            neighbors: self.neighbors,
            router_dead_interval: self.router_dead_interval,
        };

        let checksum = InternetChecksum::checksum(&Result::<Vec<u8>, ProtocolError>::from(&packet)?)?;
        packet.header = header_builder.with_checksum(checksum).build();
        Ok(packet)
    }
}

#[derive(Debug)]
pub struct OspfHelloPacket {
    header: OspfHeader,
<<<<<<< Updated upstream
    network_mask: Ipv4Netmask,
    hello_interval: u16,
    options: u8,
    router_priority: u8,
    router_dead_interval: u32,
    designated_router: Ipv4Addr,
    backup_designated_router: Ipv4Addr,
    neighbors: Vec<Ipv4Addr>,
}
impl OspfHelloPacket {
    pub fn header(&self) -> &OspfHeader {
        &self.header
    }

    pub fn network_mask(&self) -> &Ipv4Netmask {
        &self.network_mask
    }

    pub fn hello_interval(&self) -> u16 {
        self.hello_interval
    }

    pub fn options(&self) -> u8 {
        self.options
    }

    pub fn router_priority(&self) -> u8 {
        self.router_priority
    }

    pub fn router_dead_interval(&self) -> u32 {
        self.router_dead_interval
    }

    pub fn designated_router(&self) -> Ipv4Addr {
        self.designated_router
    }

    pub fn backup_designated_router(&self) -> Ipv4Addr {
        self.backup_designated_router
    }

    pub fn neighbors(&self) -> &[Ipv4Addr] {
        self.neighbors.as_ref()
    }
=======
    network_mask: u32,
    hello_interval: u16,
    options: u8,
    router_priority: u8,
    router_dead_interval: u16,
    designated_router: u32,
    backup_designated_router: u32,
    neighbors: Vec<u32>,
>>>>>>> Stashed changes
}
impl TryFrom<&[u8]> for OspfHelloPacket {
    type Error = ProtocolError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let header = OspfHeader::try_from(data)?;

        let length = header.get_length();
        if length < 44 {
            return Err(ProtocolError::InvalidMinimumLength(
                PROTOCOL,
                44,
                length as usize,
            ));
        }
        if length % 4 != 0 {
            return Err(ProtocolError::InvalidBitBoundary(PROTOCOL, length as usize));
        }
        if header.get_authentication_type() != &OspfAuthenticationType::Null {
            return Err(ProtocolError::UnsupportedFieldValue(
                PROTOCOL,
                "AuthType",
                u16::from(header.get_authentication_type()) as usize,
            ));
        }

        let network_mask = Ipv4Netmask::new(from_be_bytes!(u32, PROTOCOL, &data[24..28]));
        let hello_interval = from_be_bytes!(u16, PROTOCOL, &data[28..30]);
        let options = *&data[30];
        let router_priority = *&data[31];
        let router_dead_interval = from_be_bytes!(u32, PROTOCOL, &data[32..36]);
        let designated_router = Ipv4Addr::from(from_be_bytes!(u32, PROTOCOL, &data[36..40]));
        let backup_designated_router = Ipv4Addr::from(from_be_bytes!(u32, PROTOCOL, &data[40..44]));

        let mut neighbors: Vec<Ipv4Addr> = vec![];

        let mut i = 44usize;
        while i < length as usize {
            neighbors.push(from_be_bytes!(u32, PROTOCOL, &data[i..i + 4]).into());
            i += 4;
        }

        Ok(OspfHelloPacket {
            header,
            options,
            backup_designated_router,
            designated_router,
            neighbors,
            router_dead_interval,
            router_priority,
            hello_interval,
            network_mask,
        })
    }
}
impl From<&OspfHelloPacket> for Result<Vec<u8>, ProtocolError> {
    fn from(value: &OspfHelloPacket) -> Self {
        let mut payload = Vec::<u8>::from(value.header());

        payload.extend(value.network_mask().netmask_number().to_be_bytes());
        payload.extend(value.hello_interval().to_be_bytes());
        payload.push(value.options());
        payload.push(value.router_priority());
        payload.extend(value.router_dead_interval().to_be_bytes());
        payload.extend(value.designated_router().octets());
        payload.extend(value.backup_designated_router().octets());

        for neighbor in value.neighbors() {
            payload.extend(neighbor.octets());
        }

        Ok(payload)
    }
}
