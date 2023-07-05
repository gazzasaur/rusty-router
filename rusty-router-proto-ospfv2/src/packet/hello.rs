use std::{convert::{TryFrom, TryInto}, net::Ipv4Addr};
use crate::packet::OspfAuthenticationType;

use super::header::{PROTOCOL, OspfHeader};
use rusty_router_proto_common::{error::ProtocolError, from_be_bytes};
use rusty_router_proto_ip::Ipv4Netmask;

#[derive(Debug)]
pub struct OspfHelloPacket {
    header: OspfHeader,
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
}
impl TryFrom<&[u8]> for OspfHelloPacket {
    type Error = ProtocolError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let header = OspfHeader::try_from(data)?;
        let _data = &data[..header.get_length() as usize];

        let length = header.get_length();
        if length < 44 {
            return Err(ProtocolError::InvalidMinimumLength(PROTOCOL, 44, length as usize));
        }
        if length%4 != 0 {
            return Err(ProtocolError::InvalidBitBoundary(PROTOCOL, length as usize));
        }
        if header.get_authentication_type() != &OspfAuthenticationType::Null {
            return Err(ProtocolError::UnsupportedFieldValue(PROTOCOL, "AuthType", u16::from(header.get_authentication_type()) as usize));
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
            neighbors.push(from_be_bytes!(u32, PROTOCOL, &data[i..i+4]).into());
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
