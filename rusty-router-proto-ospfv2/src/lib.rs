use thiserror::Error;
use std::convert::TryFrom;
use std::convert::TryInto;

#[derive(Error, Debug)]
pub enum OspfParseError {
    #[error("Expecting header to be 24 bytes but only {0} bytes were read")]
    InvalidHeaderLength(u8),
    #[error("Expected version 2 but found {0}")]
    InvalidVersion(u8),
    #[error("The message type {0} is not a valid OSPFv2 message type")]
    InvalidType(u8),
    #[error("The authentication type {0} is not supported")]
    UnsupportedAuthenticationType(u16),
    #[error("Failed to perform byte conversion {0}:{1}")]
    ConversionError(&'static str, u32),
    #[error("Checksum mismatch.  Expected: {expected:#06x}, Actual: {actual:#06x}")]
    InvalidChecksum { expected: u16, actual: u16 },
}

#[derive(Debug, PartialEq)]
pub enum OspfVersion {
    V2                          // 2
}
impl TryFrom<u8> for OspfVersion {
    type Error = OspfParseError;

    fn try_from(version: u8) -> Result<Self, Self::Error> {
        if version == 2 { return Ok(Self::V2) };
        Err(OspfParseError::InvalidVersion(version))
    }
}

#[derive(Debug, PartialEq)]
pub enum OspfMessageType {
    Hello,                      // 1
    DatabaseDescription,        // 2
    LinkStateRequest,           // 3
    LinkStateUpdate,            // 4
    LinkStateAcknoledgement,    // 5
}
impl TryFrom<u8> for OspfMessageType {
    type Error = OspfParseError;

    fn try_from(message_type: u8) -> Result<Self, Self::Error> {
        if message_type == 1 { return Ok(Self::Hello) };
        if message_type == 2 { return Ok(Self::DatabaseDescription) };
        if message_type == 3 { return Ok(Self::LinkStateRequest) };
        if message_type == 4 { return Ok(Self::LinkStateUpdate) };
        if message_type == 5 { return Ok(Self::LinkStateAcknoledgement) };
        Err(OspfParseError::InvalidType(message_type))
    }
}

#[derive(Debug, PartialEq)]
pub enum OspfAuthenticationType {
    Null,                        // 0
    SimplePassword,              // 1
    CryptographicAuthentication, // 2
    // Others are available assigned by IANA but are outside the scope of the RFC
}
impl TryFrom<u16> for OspfAuthenticationType {
    type Error = OspfParseError;

    fn try_from(authentication_type: u16) -> Result<Self, Self::Error> {
        if authentication_type == 0 { return Ok(Self::Null) };
        if authentication_type == 1 { return Ok(Self::SimplePassword) };
        if authentication_type == 2 { return Ok(Self::CryptographicAuthentication) };
        Err(OspfParseError::UnsupportedAuthenticationType(authentication_type))
    }
}

#[derive(Debug, PartialEq)]
pub enum OspfAuthenticationHeader {
    Null,
    SimplePassword([u8; 8]),
    CryptographicAuthentication{
        key_id: u8,
        authentication_data_length: u8,
        cryptographic_sequence_number: u32,
    },
}
impl OspfAuthenticationHeader {
    pub fn new(authentication_type: &OspfAuthenticationType, header: &[u8; 8]) -> Result<OspfAuthenticationHeader, OspfParseError> {
        Ok (match authentication_type {
            OspfAuthenticationType::Null => OspfAuthenticationHeader::Null,
            OspfAuthenticationType::SimplePassword => OspfAuthenticationHeader::SimplePassword(header.clone()),
            OspfAuthenticationType::CryptographicAuthentication => OspfAuthenticationHeader::CryptographicAuthentication{
                key_id: header[2],
                authentication_data_length: header[3],
                cryptographic_sequence_number: u32::from_be_bytes(header[4..8].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?),
            },
        })
    }
}

#[derive(Debug)]
pub struct OspfHeader {
    version: OspfVersion,
    message_type: OspfMessageType,
    packet_length: u16,
    router_id: u32,
    area_id: u32,
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

    pub fn get_router_id(&self) -> u32 {
        self.router_id
    }

    pub fn get_area_id(&self) -> u32 {
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
    type Error = OspfParseError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        // If less than the standard header size of 24 bytes, fail fast.
        if data.len() < 24 {
            return Err(OspfParseError::InvalidHeaderLength(data.len() as u8));
        }
        let authentication_type = OspfAuthenticationType::try_from(u16::from_be_bytes(data[14..16].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?))?;

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
                checksum_aggregator += (data[i] as u64) << (8 * (1 - i%2)) as u64;
            }
            checksum_aggregator += checksum_aggregator >> 16;
            checksum_aggregator ^= 0x000000000000FFFF;
        }
        let checksum = u16::from_be_bytes(data[12..14].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?);
        let expected_checksum = (checksum_aggregator & 0x000000000000FFFF).try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?;
        if checksum != expected_checksum {
            return Err(OspfParseError::InvalidChecksum { expected: expected_checksum, actual: checksum });
        }

        Ok(OspfHeader {
            version: OspfVersion::try_from(data[0])?,
            message_type: OspfMessageType::try_from(data[1])?,
            packet_length: u16::from_be_bytes(data[2..4].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?),
            router_id: u32::from_be_bytes(data[4..8].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?),
            area_id: u32::from_be_bytes(data[8..12].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?),
            authentication_header: OspfAuthenticationHeader::new(&authentication_type, data[16..24].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?)?,
            authentication_type,
            checksum,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::result::Result;
    use std::convert::TryFrom;

    #[test]
    fn it_parses_version() -> Result<(), OspfParseError> {
        assert_eq!(OspfVersion::try_from(2)?, OspfVersion::V2);
        Ok(())
    }

    #[test]
    fn it_fails_to_parse_version() {
        if let Err(e) = OspfVersion::try_from(5) {
            assert_eq!(e.to_string(), "Expected version 2 but found 5".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn it_parses_type() -> Result<(), OspfParseError> {
        assert_eq!(OspfMessageType::try_from(1)?, OspfMessageType::Hello);
        assert_eq!(OspfMessageType::try_from(2)?, OspfMessageType::DatabaseDescription);
        assert_eq!(OspfMessageType::try_from(3)?, OspfMessageType::LinkStateRequest);
        assert_eq!(OspfMessageType::try_from(4)?, OspfMessageType::LinkStateUpdate);
        assert_eq!(OspfMessageType::try_from(5)?, OspfMessageType::LinkStateAcknoledgement);
        Ok(())
    }

    #[test]
    fn it_fails_to_parse_type() {
        if let Err(e) = OspfMessageType::try_from(0) {
            assert_eq!(e.to_string(), "The message type 0 is not a valid OSPFv2 message type".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn it_parses_authn_type() -> Result<(), OspfParseError> {
        assert_eq!(OspfAuthenticationType::try_from(0)?, OspfAuthenticationType::Null);
        assert_eq!(OspfAuthenticationType::try_from(1)?, OspfAuthenticationType::SimplePassword);
        assert_eq!(OspfAuthenticationType::try_from(2)?, OspfAuthenticationType::CryptographicAuthentication);
        Ok(())
    }

    #[test]
    fn it_fails_to_parse_authn_type() {
        if let Err(e) = OspfAuthenticationType::try_from(10) {
            assert_eq!(e.to_string(), "The authentication type 10 is not supported".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn it_parses_authn_header() -> Result<(), OspfParseError> {
        assert_eq!(OspfAuthenticationHeader::new(&OspfAuthenticationType::Null, &[34, 12, 98, 234, 1, 0, 22, 4])?, OspfAuthenticationHeader::Null);
        assert_eq!(OspfAuthenticationHeader::new(&OspfAuthenticationType::SimplePassword, &[34, 12, 98, 234, 1, 0, 22, 4])?, OspfAuthenticationHeader::SimplePassword([34, 12, 98, 234, 1, 0, 22, 4]));
        assert_ne!(OspfAuthenticationHeader::new(&OspfAuthenticationType::SimplePassword, &[34, 12, 98, 234, 1, 0, 21, 4])?, OspfAuthenticationHeader::SimplePassword([34, 12, 98, 234, 1, 0, 22, 4]));
        assert_eq!(OspfAuthenticationHeader::new(&OspfAuthenticationType::CryptographicAuthentication, &[34, 12, 98, 234, 254, 0, 22, 4])?, OspfAuthenticationHeader::CryptographicAuthentication {
            key_id: 98,
            authentication_data_length: 234,
            cryptographic_sequence_number: 4261418500,
        });
        assert_ne!(OspfAuthenticationHeader::new(&OspfAuthenticationType::CryptographicAuthentication, &[34, 12, 98, 234, 62, 0, 22, 4])?, OspfAuthenticationHeader::CryptographicAuthentication {
            key_id: 98,
            authentication_data_length: 234,
            cryptographic_sequence_number: 4261418500,
        });
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_happy_null_auth() -> Result<(), OspfParseError> {
        let subject = OspfHeader::try_from(&[
            2, 1, 0, 0,
            127, 0, 0, 1,
            0, 0, 0, 1,
            126, 252, 0, 0,
            0, 0, 0, 0,
            0, 0, 0, 0,
        ][..])?;
        assert_eq!(subject.get_version(), &OspfVersion::V2);
        assert_eq!(subject.get_type(), &OspfMessageType::Hello);
        assert_eq!(subject.get_length(), 0);
        assert_eq!(subject.get_router_id(), 2130706433);
        assert_eq!(subject.get_area_id(), 1);
        assert_eq!(subject.get_checksum(), 32508);
        assert_eq!(subject.get_authentication_type(), &OspfAuthenticationType::Null);
        assert_eq!(subject.get_authentication_header(), &OspfAuthenticationHeader::Null);
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_too_short_sad() -> Result<(), OspfParseError> {
        if let Err(e) = OspfHeader::try_from(&[
            2, 1, 0, 0,
            127, 0, 0, 1,
            0, 0, 0, 1,
            126, 252, 0, 0,
            0, 0, 0, 0,
            0, 0, 0,
        ][..]) {
            assert_eq!(e.to_string(), "Expecting header to be 24 bytes but only 23 bytes were read".to_string());
        } else {
            assert!(false, "Result was expected to be an Err");
        }
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_happy_simple_auth() -> Result<(), OspfParseError> {
        let subject = OspfHeader::try_from(&[
            2, 1, 0, 1,
            127, 0, 0, 1,
            0, 0, 0, 1,
            126, 250, 0, 1,
            10, 11, 12, 13,
            14, 15, 16, 17,
        ][..])?;
        assert_eq!(subject.get_version(), &OspfVersion::V2);
        assert_eq!(subject.get_type(), &OspfMessageType::Hello);
        assert_eq!(subject.get_length(), 1);
        assert_eq!(subject.get_router_id(), 2130706433);
        assert_eq!(subject.get_area_id(), 1);
        assert_eq!(subject.get_checksum(), 32506);
        assert_eq!(subject.get_authentication_type(), &OspfAuthenticationType::SimplePassword);
        assert_eq!(subject.get_authentication_header(), &OspfAuthenticationHeader::SimplePassword([10, 11, 12, 13, 14, 15, 16, 17]));
        Ok(())
    }

    #[test]
    fn it_parse_ospf_header_happy_crypto_auth() -> Result<(), OspfParseError> {
        let subject = OspfHeader::try_from(&[
            2, 1, 0, 0,
            127, 0, 0, 1,
            0, 0, 0, 1,
            0, 0, 0, 2,
            10, 11, 12, 13,
            14, 15, 16, 17,
        ][..])?;
        assert_eq!(subject.get_version(), &OspfVersion::V2);
        assert_eq!(subject.get_type(), &OspfMessageType::Hello);
        assert_eq!(subject.get_length(), 0);
        assert_eq!(subject.get_router_id(), 2130706433);
        assert_eq!(subject.get_area_id(), 1);
        assert_eq!(subject.get_checksum(), 0);
        assert_eq!(subject.get_authentication_type(), &OspfAuthenticationType::CryptographicAuthentication);
        assert_eq!(subject.get_authentication_header(), &OspfAuthenticationHeader::CryptographicAuthentication {
            key_id: 12,
            authentication_data_length: 13,
            cryptographic_sequence_number: 235868177,
        });
        Ok(())
    }
}
