use rusty_router_proto_common::from_be_bytes;
use rusty_router_proto_common::prelude::*;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::net::Ipv4Addr;

pub const PROTOCOL: &'static str = "OspfV2";
pub const OSPF_V2_HEADER_LENGTH: u16 = 24;

#[derive(Debug, PartialEq)]
pub enum OspfVersion {
    V2, // 2
}
impl TryFrom<u8> for OspfVersion {
    type Error = ProtocolError;

    fn try_from(version: u8) -> Result<Self> {
        if version == 2 {
            return Ok(Self::V2);
        };
        Err(ProtocolError::UnsupportedFieldValue(
            PROTOCOL,
            "Version",
            version as usize,
        ))
    }
}
impl From<&OspfVersion> for u8 {
    fn from(version: &OspfVersion) -> Self {
        match version {
            OspfVersion::V2 => 2u8,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OspfMessageType {
    Hello,                   // 1
    DatabaseDescription,     // 2
    LinkStateRequest,        // 3
    LinkStateUpdate,         // 4
    LinkStateAcknoledgement, // 5
}
impl TryFrom<u8> for OspfMessageType {
    type Error = ProtocolError;

    fn try_from(message_type: u8) -> Result<Self> {
        if message_type == 1 {
            return Ok(Self::Hello);
        };
        if message_type == 2 {
            return Ok(Self::DatabaseDescription);
        };
        if message_type == 3 {
            return Ok(Self::LinkStateRequest);
        };
        if message_type == 4 {
            return Ok(Self::LinkStateUpdate);
        };
        if message_type == 5 {
            return Ok(Self::LinkStateAcknoledgement);
        };
        Err(ProtocolError::UnsupportedFieldValue(
            PROTOCOL,
            "Type",
            message_type as usize,
        ))
    }
}
impl From<&OspfMessageType> for u8 {
    fn from(value: &OspfMessageType) -> Self {
        match value {
            OspfMessageType::Hello => 1u8,
            OspfMessageType::DatabaseDescription => 2u8,
            OspfMessageType::LinkStateRequest => 3u8,
            OspfMessageType::LinkStateUpdate => 4u8,
            OspfMessageType::LinkStateAcknoledgement => 5u8,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum OspfAuthenticationType {
    Null,           // 0
    SimplePassword, // 1
    CryptographicAuthentication, // 2
                    // Others are available assigned by IANA but are outside the scope of the RFC
}
impl TryFrom<u16> for OspfAuthenticationType {
    type Error = ProtocolError;

    fn try_from(authentication_type: u16) -> Result<Self> {
        if authentication_type == 0 {
            return Ok(Self::Null);
        };
        if authentication_type == 1 {
            return Ok(Self::SimplePassword);
        };
        if authentication_type == 2 {
            return Ok(Self::CryptographicAuthentication);
        };
        Err(ProtocolError::UnsupportedFieldValue(
            PROTOCOL,
            "AuthenticationType",
            authentication_type as usize,
        ))
    }
}
impl From<&OspfAuthenticationType> for u16 {
    fn from(authentication_type: &OspfAuthenticationType) -> Self {
        match authentication_type {
            &OspfAuthenticationType::Null => 0,
            &OspfAuthenticationType::SimplePassword => 1,
            &OspfAuthenticationType::CryptographicAuthentication => 2,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum OspfAuthenticationHeader {
    Null,
    SimplePassword([u8; 8]),
    CryptographicAuthentication {
        key_id: u8,
        authentication_data_length: u8,
        cryptographic_sequence_number: u32,
    },
}
impl OspfAuthenticationHeader {
    pub fn new(
        authentication_type: &OspfAuthenticationType,
        header: &[u8; 8],
    ) -> Result<OspfAuthenticationHeader> {
        Ok(match authentication_type {
            OspfAuthenticationType::Null => OspfAuthenticationHeader::Null,
            OspfAuthenticationType::SimplePassword => {
                OspfAuthenticationHeader::SimplePassword(header.clone())
            }
            OspfAuthenticationType::CryptographicAuthentication => {
                OspfAuthenticationHeader::CryptographicAuthentication {
                    key_id: header[2],
                    authentication_data_length: header[3],
                    cryptographic_sequence_number: from_be_bytes!(u32, PROTOCOL, header[4..8]),
                }
            }
        })
    }
}
impl From<&OspfAuthenticationHeader> for u64 {
    fn from(value: &OspfAuthenticationHeader) -> Self {
        match value {
            OspfAuthenticationHeader::Null => 0u64,
            OspfAuthenticationHeader::SimplePassword(data) => u64::from_be_bytes(data.clone()),
            OspfAuthenticationHeader::CryptographicAuthentication {
                key_id,
                authentication_data_length,
                cryptographic_sequence_number,
            } => {
                let mut res = [0u8; 8];
                res[2] = *key_id;
                res[3] = *authentication_data_length;
                let crypto_data = cryptographic_sequence_number.to_be_bytes();
                res[4] = crypto_data[0];
                res[5] = crypto_data[1];
                res[6] = crypto_data[2];
                res[7] = crypto_data[3];
                u64::from_be_bytes(res)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct OspfHeaderBuilder {
    message_type: OspfMessageType,
    packet_length: u16,
    router_id: Ipv4Addr,
    area_id: Ipv4Addr,
    checksum: u16,
}
impl OspfHeaderBuilder {
    pub fn new(
        message_type: OspfMessageType,
        packet_length: u16,
        router_id: Ipv4Addr,
        area_id: Ipv4Addr,
    ) -> Self {
        OspfHeaderBuilder {
            message_type,
            packet_length,
            router_id,
            area_id,
            checksum: 0u16,
        }
    }

    pub fn with_checksum(mut self, checksum: u16) -> Self {
        self.checksum = checksum;
        self
    }

    pub fn build(self) -> OspfHeader {
        OspfHeader {
            version: OspfVersion::V2,
            message_type: self.message_type,
            packet_length: self.packet_length,
            router_id: self.router_id,
            area_id: self.area_id,
            checksum: self.checksum,
            authentication_type: OspfAuthenticationType::Null,
            authentication_header: OspfAuthenticationHeader::Null
        }
    }
}

#[derive(Debug)]
pub struct OspfHeader {
    version: OspfVersion,
    message_type: OspfMessageType,
    packet_length: u16,
    router_id: Ipv4Addr,
    area_id: Ipv4Addr,
    checksum: u16,
    authentication_type: OspfAuthenticationType,
    authentication_header: OspfAuthenticationHeader,
}
impl OspfHeader {
    pub fn get_version(&self) -> &OspfVersion {
        &self.version
    }

    pub fn get_type(&self) -> &OspfMessageType {
        &self.message_type
    }

    pub fn get_length(&self) -> u16 {
        self.packet_length
    }

    pub fn get_router_id(&self) -> Ipv4Addr {
        self.router_id
    }

    pub fn get_area_id(&self) -> Ipv4Addr {
        self.area_id
    }

    pub fn get_checksum(&self) -> u16 {
        self.checksum
    }

    pub fn get_authentication_type(&self) -> &OspfAuthenticationType {
        &self.authentication_type
    }

    pub fn get_authentication_header(&self) -> &OspfAuthenticationHeader {
        &self.authentication_header
    }
}
impl TryFrom<&[u8]> for OspfHeader {
    type Error = ProtocolError;

    fn try_from(data: &[u8]) -> Result<Self> {
        // If less than the standard header size of 24 bytes, fail fast.
        if data.len() < 24 {
            return Err(ProtocolError::InvalidHeaderLength(PROTOCOL, 24, data.len()));
        }

        // If the header packet length is too short, also fail
        let packet_length = from_be_bytes!(u16, PROTOCOL, data[2..4]);
        if packet_length < 24 {
            return Err(ProtocolError::InvalidHeaderLength(PROTOCOL, 24, data.len()));
        }
        // We do not care if the packet is too long, simply truncate, but if it is too short, we have a problem
        if data.len() < packet_length as usize {
            return Err(ProtocolError::InvalidMinimumLength(
                PROTOCOL,
                packet_length as usize,
                data.len(),
            ));
        }
        // Ensure the packet is aligned with a word boundary
        if packet_length % 4 != 0 {
            return Err(ProtocolError::InvalidBitBoundary(
                PROTOCOL,
                packet_length as usize,
            ));
        }
        let data = &data[..packet_length as usize];
        let authentication_type =
            OspfAuthenticationType::try_from(from_be_bytes!(u16, PROTOCOL, data[14..16]))?;

        let mut checksum_aggregator: u64 = 0;
        // The checksum is zero for Cryptographic Authentication
        if authentication_type != OspfAuthenticationType::CryptographicAuthentication {
            for i in 0..data.len() {
                // Skip checksum and authentication header
                //
                // TODO This if statement is performed per byte.
                // It can be removed completely if AuthN is extracted and set to 0
                // and changing the checksum to the standard recommended comparison against 0xFFFF
                if i >= 12 && i < 14 || i >= 16 && i < 24 {
                    continue;
                }
                checksum_aggregator += (data[i] as u64) << (8 * (1 - i % 2)) as u64;
            }
            checksum_aggregator += checksum_aggregator >> 16;
            checksum_aggregator ^= 0x000000000000FFFF;
        }
        let checksum = from_be_bytes!(u16, PROTOCOL, data[12..14]);
        let expected_checksum = (checksum_aggregator & 0x000000000000FFFF) as u16;
        if checksum != expected_checksum {
            return Err(ProtocolError::InvalidChecksum {
                proto: PROTOCOL,
                expected: expected_checksum,
                actual: checksum,
            });
        }

        Ok(OspfHeader {
            version: OspfVersion::try_from(data[0])?,
            message_type: OspfMessageType::try_from(data[1])?,
            router_id: Ipv4Addr::from(from_be_bytes!(u32, PROTOCOL, data[4..8])),
            area_id: Ipv4Addr::from(from_be_bytes!(u32, PROTOCOL, data[8..12])),
            authentication_header: OspfAuthenticationHeader::new(
                &authentication_type,
                data[16..24]
                    .try_into()
                    .map_err(|_| ProtocolError::ConversionError(PROTOCOL, file!(), line!()))?,
            )?,
            authentication_type,
            packet_length,
            checksum,
        })
    }
}
impl From<&OspfHeader> for Vec<u8> {
    fn from(header: &OspfHeader) -> Self {
        let mut buffer: Vec<u8> = Vec::new();

        buffer.push(header.get_version().into());
        buffer.push(header.get_type().into());
        buffer.extend_from_slice(&header.get_length().to_be_bytes());
        buffer.extend_from_slice(&header.get_router_id().octets());
        buffer.extend_from_slice(&header.get_area_id().octets());
        buffer.extend_from_slice(&header.get_checksum().to_be_bytes());
        buffer.extend_from_slice(&u16::from(header.get_authentication_type()).to_be_bytes());
        buffer.extend_from_slice(&u64::from(header.get_authentication_header()).to_be_bytes());

        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;
    use std::result::Result;

    #[test]
    fn it_parses_version() -> Result<(), ProtocolError> {
        assert_eq!(OspfVersion::try_from(2)?, OspfVersion::V2);
        Ok(())
    }

    #[test]
    fn it_fails_to_parse_version() {
        if let Err(e) = OspfVersion::try_from(5) {
            assert_eq!(
                e.to_string(),
                "[OspfV2] The value '5' for field 'Version' is not supported".to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn it_parses_type() -> Result<(), ProtocolError> {
        assert_eq!(OspfMessageType::try_from(1)?, OspfMessageType::Hello);
        assert_eq!(
            OspfMessageType::try_from(2)?,
            OspfMessageType::DatabaseDescription
        );
        assert_eq!(
            OspfMessageType::try_from(3)?,
            OspfMessageType::LinkStateRequest
        );
        assert_eq!(
            OspfMessageType::try_from(4)?,
            OspfMessageType::LinkStateUpdate
        );
        assert_eq!(
            OspfMessageType::try_from(5)?,
            OspfMessageType::LinkStateAcknoledgement
        );
        Ok(())
    }

    #[test]
    fn it_fails_to_parse_type() {
        if let Err(e) = OspfMessageType::try_from(0) {
            assert_eq!(
                e.to_string(),
                "[OspfV2] The value '0' for field 'Type' is not supported".to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn it_parses_authn_type() -> Result<(), ProtocolError> {
        assert_eq!(
            OspfAuthenticationType::try_from(0)?,
            OspfAuthenticationType::Null
        );
        assert_eq!(
            OspfAuthenticationType::try_from(1)?,
            OspfAuthenticationType::SimplePassword
        );
        assert_eq!(
            OspfAuthenticationType::try_from(2)?,
            OspfAuthenticationType::CryptographicAuthentication
        );
        Ok(())
    }

    #[test]
    fn it_fails_to_parse_authn_type() {
        if let Err(e) = OspfAuthenticationType::try_from(10) {
            assert_eq!(
                e.to_string(),
                "[OspfV2] The value '10' for field 'AuthenticationType' is not supported"
                    .to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn it_parses_authn_header() -> Result<(), ProtocolError> {
        assert_eq!(
            OspfAuthenticationHeader::new(
                &OspfAuthenticationType::Null,
                &[34, 12, 98, 234, 1, 0, 22, 4]
            )?,
            OspfAuthenticationHeader::Null
        );
        assert_eq!(
            OspfAuthenticationHeader::new(
                &OspfAuthenticationType::SimplePassword,
                &[34, 12, 98, 234, 1, 0, 22, 4]
            )?,
            OspfAuthenticationHeader::SimplePassword([34, 12, 98, 234, 1, 0, 22, 4])
        );
        assert_ne!(
            OspfAuthenticationHeader::new(
                &OspfAuthenticationType::SimplePassword,
                &[34, 12, 98, 234, 1, 0, 21, 4]
            )?,
            OspfAuthenticationHeader::SimplePassword([34, 12, 98, 234, 1, 0, 22, 4])
        );
        assert_eq!(
            OspfAuthenticationHeader::new(
                &OspfAuthenticationType::CryptographicAuthentication,
                &[34, 12, 98, 234, 254, 0, 22, 4]
            )?,
            OspfAuthenticationHeader::CryptographicAuthentication {
                key_id: 98,
                authentication_data_length: 234,
                cryptographic_sequence_number: 4261418500,
            }
        );
        assert_ne!(
            OspfAuthenticationHeader::new(
                &OspfAuthenticationType::CryptographicAuthentication,
                &[34, 12, 98, 234, 62, 0, 22, 4]
            )?,
            OspfAuthenticationHeader::CryptographicAuthentication {
                key_id: 98,
                authentication_data_length: 234,
                cryptographic_sequence_number: 4261418500,
            }
        );
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_happy_null_auth() -> Result<(), ProtocolError> {
        let subject = OspfHeader::try_from(
            &[
                2, 1, 0, 24, 127, 0, 0, 1, 0, 0, 0, 1, 126, 228, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ][..],
        )?;
        assert_eq!(subject.get_version(), &OspfVersion::V2);
        assert_eq!(subject.get_type(), &OspfMessageType::Hello);
        assert_eq!(subject.get_length(), 24);
        assert_eq!(subject.get_router_id(), Ipv4Addr::from(2130706433));
        assert_eq!(subject.get_area_id(), Ipv4Addr::from(1));
        assert_eq!(subject.get_checksum(), 32484);
        assert_eq!(
            subject.get_authentication_type(),
            &OspfAuthenticationType::Null
        );
        assert_eq!(
            subject.get_authentication_header(),
            &OspfAuthenticationHeader::Null
        );
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_happy_null_auth_longer_packet() -> Result<(), ProtocolError> {
        let subject = OspfHeader::try_from(
            &[
                2, 1, 0, 24, 127, 0, 0, 1, 0, 0, 0, 1, 126, 228, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
                2, 3, 4, 5,
            ][..],
        )?;
        assert_eq!(subject.get_version(), &OspfVersion::V2);
        assert_eq!(subject.get_type(), &OspfMessageType::Hello);
        assert_eq!(subject.get_length(), 24);
        assert_eq!(subject.get_router_id(), Ipv4Addr::from(2130706433));
        assert_eq!(subject.get_area_id(), Ipv4Addr::from(1));
        assert_eq!(subject.get_checksum(), 32484);
        assert_eq!(
            subject.get_authentication_type(),
            &OspfAuthenticationType::Null
        );
        assert_eq!(
            subject.get_authentication_header(),
            &OspfAuthenticationHeader::Null
        );
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_too_short_sad() -> Result<(), ProtocolError> {
        if let Err(e) = OspfHeader::try_from(
            &[
                2, 1, 0, 24, 127, 0, 0, 1, 0, 0, 0, 1, 126, 228, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ][..],
        ) {
            assert_eq!(
                e.to_string(),
                "[OspfV2] Expecting header to be 24 bytes but only 23 bytes are available"
                    .to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
        Ok(())
    }

    #[test]
    fn it_parse_ospf_packet_too_short() -> Result<(), ProtocolError> {
        if let Err(e) = OspfHeader::try_from(
            &[
                2, 1, 1, 0, 127, 0, 0, 1, 0, 0, 0, 1, 126, 228, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ][..],
        ) {
            assert_eq!(
                e.to_string(),
                "[OspfV2] Expecting data of at least 256 bytes but only 24 bytes are available"
                    .to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_happy_simple_auth() -> Result<(), ProtocolError> {
        let subject = OspfHeader::try_from(
            &[
                2, 1, 0, 24, 127, 0, 0, 1, 0, 0, 0, 1, 126, 227, 0, 1, 10, 11, 12, 13, 14, 15, 16,
                17,
            ][..],
        )?;
        assert_eq!(subject.get_version(), &OspfVersion::V2);
        assert_eq!(subject.get_type(), &OspfMessageType::Hello);
        assert_eq!(subject.get_length(), 24);
        assert_eq!(subject.get_router_id(), Ipv4Addr::from(2130706433));
        assert_eq!(subject.get_area_id(), Ipv4Addr::from(1));
        assert_eq!(subject.get_checksum(), 32483);
        assert_eq!(
            subject.get_authentication_type(),
            &OspfAuthenticationType::SimplePassword
        );
        assert_eq!(
            subject.get_authentication_header(),
            &OspfAuthenticationHeader::SimplePassword([10, 11, 12, 13, 14, 15, 16, 17])
        );
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_happy_crypto_auth() -> Result<(), ProtocolError> {
        let subject = OspfHeader::try_from(
            &[
                2, 1, 0, 24, 127, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 2, 10, 11, 12, 13, 14, 15, 16, 17,
            ][..],
        )?;
        assert_eq!(subject.get_version(), &OspfVersion::V2);
        assert_eq!(subject.get_type(), &OspfMessageType::Hello);
        assert_eq!(subject.get_length(), 24);
        assert_eq!(subject.get_router_id(), Ipv4Addr::from(2130706433));
        assert_eq!(subject.get_area_id(), Ipv4Addr::from(1));
        assert_eq!(subject.get_checksum(), 0);
        assert_eq!(
            subject.get_authentication_type(),
            &OspfAuthenticationType::CryptographicAuthentication
        );
        assert_eq!(
            subject.get_authentication_header(),
            &OspfAuthenticationHeader::CryptographicAuthentication {
                key_id: 12,
                authentication_data_length: 13,
                cryptographic_sequence_number: 235868177,
            }
        );
        Ok(())
    }
}
