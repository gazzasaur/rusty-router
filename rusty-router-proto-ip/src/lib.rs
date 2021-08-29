use std::convert::TryFrom;
use std::convert::TryInto;

use rusty_router_proto_common::{from_be_bytes, ProtocolParseError};

pub const PROTO: &'static str = "IPv4";

#[derive(Debug, PartialEq)]
pub enum IpVersion {
    IPv4,
}

pub struct IpV4Header {
    version: IpVersion,
    header_length: u16,
    type_of_service: u8,
    total_length: u16,

    identification: u16,
    flags: u8,
    fragment_offset: u16,

    time_to_live: u8,
    protocol: u8,
    header_checksum: u16,
    source_address: u32,
    destination_address: u32,

    // TODO Do not need to process options for now    
}
impl IpV4Header {
    pub fn get_version(&self) -> &IpVersion {
        &self.version
    }

    pub fn get_header_length(&self) -> u16 {
        self.header_length
    }

    pub fn get_type_of_service(&self) -> u8 {
        self.type_of_service
    }

    pub fn get_total_length(&self) -> u16 {
        self.total_length
    }

    pub fn get_identification(&self) -> u16 {
        self.identification
    }

    pub fn get_flags(&self) -> u8 {
        self.flags
    }

    pub fn get_fragment_offset(&self) -> u16 {
        self.fragment_offset
    }

    pub fn get_time_to_live(&self) -> u8 {
        self.time_to_live
    }

    pub fn get_protocol(&self) -> u8 {
        self.protocol
    }

    pub fn get_header_checksum(&self) -> u16 {
        self.header_checksum
    }

    pub fn get_source_address(&self) -> u32 {
        self.source_address
    }

    pub fn get_destination_address(&self) -> u32 {
        self.destination_address
    }
}
impl TryFrom<&[u8]> for IpV4Header {
    type Error = ProtocolParseError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        // Ensure we at least have the minimum number of bytes for the header
        if data.len() < 20 {
            return Err(ProtocolParseError::InvalidMinimumLength(PROTO, 20, data.len()));
        }

        let version = (data[0] & 0xF0) >> 4;
        if version != 4 {
            return Err(ProtocolParseError::UnsupportedFieldValue(PROTO, "Version", version as usize));
        }

        let internet_header_length = data[0] & 0x0F;
        let header_length = (internet_header_length as u16) * 4;
        if header_length < 20 {
            return Err(ProtocolParseError::UnsupportedFieldValue(PROTO, "Internet Header Length", internet_header_length as usize));
        }
        if header_length as usize > data.len() {
            return Err(ProtocolParseError::InvalidMinimumLength(PROTO, header_length as usize, data.len()));
        }

        let type_of_service = data[1];
        let total_length = from_be_bytes!(u16, PROTO, &data[2..4]);
        let identification = from_be_bytes!(u16, PROTO, &data[4..6]);
        let flags = (data[6] >> 5) & 0x07;
        let fragment_offset = (from_be_bytes!(u16, PROTO, &data[6..8]) & 0x1FFF) * 8;
        let time_to_live = data[8];
        let protocol = data[9];
        let header_checksum = from_be_bytes!(u16, PROTO, &data[10..12]);
        let source_address = from_be_bytes!(u32, PROTO, &data[12..16]);
        let destination_address = from_be_bytes!(u32, PROTO, &data[16..20]);

        Ok(IpV4Header {
            version: IpVersion::IPv4,
            header_length,
            type_of_service,
            total_length,
            identification,
            flags,
            fragment_offset,
            time_to_live,
            protocol,
            header_checksum,
            source_address,
            destination_address,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_router_proto_common::InternetChecksum;

    #[test]
    fn parse_header() -> Result<(), ProtocolParseError> {
        let data = [0x45, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11];
        let header = IpV4Header::try_from(&data[..])?;
        assert_eq!(InternetChecksum::checksum(&data[..])?, 0);
        assert_eq!(header.get_version(), &IpVersion::IPv4);
        assert_eq!(header.get_header_length(), 20);
        assert_eq!(header.get_total_length(), 8760);
        assert_eq!(header.get_identification(), 20058);
        assert_eq!(header.get_flags(), 5);
        assert_eq!(header.get_type_of_service(), 12);
        assert_eq!(header.get_fragment_offset(), 35600);
        assert_eq!(header.get_time_to_live(), 127);
        assert_eq!(header.get_protocol(), 89);
        assert_eq!(header.get_header_checksum(), 0x888F);
        assert_eq!(header.get_source_address(), 127 << 24 | 1);
        assert_eq!(header.get_destination_address(), 8 << 24 | 9 << 16 | 10 << 8 | 11);
        Ok(())
    }

    #[test]
    fn parse_small_header_sad() {
        let data = [0x45, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 1, 8, 9, 10, 11];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(e.to_string(), "[IPv4] Expecting data of at least 20 bytes but only 18 bytes are available".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn parse_insufficient_header_data_sad() {
        let data = [0x46, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(e.to_string(), "[IPv4] Expecting data of at least 24 bytes but only 20 bytes are available".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn parse_invalid_header_length_sad() {
        let data = [0x44, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(e.to_string(), "[IPv4] The value '4' for field 'Internet Header Length' is not supported".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn parse_inversion_sad() {
        let data = [0x35, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(e.to_string(), "[IPv4] The value '3' for field 'Version' is not supported".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }
}
