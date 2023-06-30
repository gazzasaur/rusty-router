use rand::Rng;
use rusty_router_proto_common::from_be_bytes;
use rusty_router_proto_common::prelude::ProtocolError;
use rusty_router_proto_common::prelude::Result;
use rusty_router_proto_common::InternetChecksum;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::net::Ipv4Addr;
use rand::thread_rng;

pub const IP_V4_PROTO: &'static str = "IPv4";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpVersion {
    version: u8,
}
impl IpVersion {
    pub fn ipv4() -> Self {
        Self { version: 4 }
    }

    pub fn ipv6() -> Self {
        Self { version: 6 }
    }

    pub fn custom(value: u8) -> Result<Self> {
        if value > 0x0F {
            Err(ProtocolError::UnsupportedFieldValue(
                IP_V4_PROTO,
                "Version",
                value as usize,
            ))
        } else {
            Ok(Self { version: value })
        }
    }

    pub fn get_value(&self) -> u8 {
        self.version
    }
}

pub enum Ipv4Flag {
    Reserved,
    DontFragment,
    MoreFragments,
}
impl Ipv4Flag {
    fn get_value(&self) -> u8 {
        match self {
            Self::Reserved => 1,
            Self::DontFragment => 2,
            Self::MoreFragments => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ipv4Flags {
    flags: u8,
}
impl Ipv4Flags {
    pub fn new(flags: &[Ipv4Flag]) -> Ipv4Flags {
        let mut ipv4_flags = 0u8;
        for flag in flags {
            ipv4_flags |= flag.get_value();
        }
        Ipv4Flags { flags: ipv4_flags }
    }

    pub fn is_set(&self, flag: Ipv4Flag) -> bool {
        (self.flags & (flag as u8)) == !0
    }
}
impl TryFrom<u8> for Ipv4Flags {
    type Error = ProtocolError;

    fn try_from(flags: u8) -> Result<Self> {
        if flags & 0xF8 != 0 {
            Err(ProtocolError::UnsupportedFieldValue(
                IP_V4_PROTO,
                "Flags",
                flags as usize,
            ))
        } else {
            Ok(Self { flags })
        }
    }
}
impl From<&Ipv4Flags> for u8 {
    fn from(value: &Ipv4Flags) -> Self {
        value.flags
    }
}

#[derive(Clone)]
pub struct IpV4HeaderBuilder {
    header: IpV4Header,
    identification: Option<u16>,
    header_checksum: Option<u16>,
    internet_header_length: Option<u8>,
}
impl IpV4HeaderBuilder {
    /**
     * Setting the identifier and checksum to zero will allow the kernel to set the values.
     * Identification will be set randomly.
     */
    pub fn new(
        source_address: Ipv4Addr,
        destination_address: Ipv4Addr,
        protocol: u8,
    ) -> IpV4HeaderBuilder {
        IpV4HeaderBuilder {
            header: IpV4Header {
                version: IpVersion::ipv4(),
                time_to_live: 128,
                protocol,
                source_address,
                destination_address,
                total_length: 0,
                identification: 0,
                fragment_offset: 0,
                header_checksum: 0,
                type_of_service: 0,
                internet_header_length: 0,
                flags: Ipv4Flags::new(&[]),
            },
            identification: None,
            header_checksum: None,
            internet_header_length: None,
        }
    }

    pub fn force_internet_header_length(
        &mut self,
        internet_header_length: Option<u8>,
    ) -> Result<&mut Self> {
        let internet_header_length = if let Some(internet_header_length) = internet_header_length {
            internet_header_length
        } else {
            self.internet_header_length = None;
            return Ok(self);
        };
        if internet_header_length > 0x0F {
            Err(ProtocolError::UnsupportedFieldValue(
                IP_V4_PROTO,
                "InternetHeaderLength",
                internet_header_length as usize,
            ))
        } else {
            self.internet_header_length = Some(internet_header_length);
            Ok(self)
        }
    }

    pub fn force_identification(&mut self, identification: Option<u16>) -> &mut Self {
        self.identification = identification;
        self
    }

    pub fn force_header_checksum(&mut self, header_checksum: Option<u16>) -> &mut Self {
        self.header_checksum = header_checksum;
        self
    }

    pub fn build(self, payload_length: usize) -> Result<IpV4Header> {
        if payload_length > (u16::MAX as usize - 20) {
            return Err(ProtocolError::InvalidLength(
                "IPv4 payload length without fragmentation",
                (u16::MAX - 20) as usize,
                payload_length as usize,
            ));
        }
        let mut header = self.header.clone();
        header.total_length = 20 + payload_length as u16;
        header.identification = match self.identification {
            Some(identification) => identification,
            None => thread_rng().gen::<u16>(),
        };
        header.internet_header_length = match self.internet_header_length {
            Some(internet_header_length) => internet_header_length,
            None => 5,
        };
        header.header_checksum = match self.header_checksum {
            Some(header_checksum) => header_checksum,
            None => InternetChecksum::checksum(&Vec::<u8>::from(&header)[..])?,
        };
        Ok(header)
    }
}

#[derive(Clone, Debug)]
pub struct IpV4Header {
    version: IpVersion,
    internet_header_length: u8,
    type_of_service: u8,
    total_length: u16,

    identification: u16,
    flags: Ipv4Flags,
    fragment_offset: u16,

    time_to_live: u8,
    protocol: u8,
    header_checksum: u16,
    source_address: Ipv4Addr,
    destination_address: Ipv4Addr,
    // Options are ignored
}
impl IpV4Header {
    pub fn get_version(&self) -> &IpVersion {
        &self.version
    }

    pub fn get_internet_header_length(&self) -> u8 {
        self.internet_header_length
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

    pub fn get_flags(&self) -> &Ipv4Flags {
        &self.flags
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

    pub fn get_source_address(&self) -> Ipv4Addr {
        self.source_address
    }

    pub fn get_destination_address(&self) -> Ipv4Addr {
        self.destination_address
    }
}
impl TryFrom<&[u8]> for IpV4Header {
    type Error = ProtocolError;

    fn try_from(data: &[u8]) -> Result<Self> {
        // Ensure we at least have the minimum number of bytes for the header
        if data.len() < 20 {
            return Err(ProtocolError::InvalidMinimumLength(
                IP_V4_PROTO,
                20,
                data.len(),
            ));
        }

        let version = (data[0] & 0xF0) >> 4;
        if version != 4 {
            return Err(ProtocolError::UnsupportedFieldValue(
                IP_V4_PROTO,
                "Version",
                version as usize,
            ));
        }

        let internet_header_length = data[0] & 0x0F;
        let header_length = (internet_header_length as u16) * 4;
        if header_length < 20 {
            return Err(ProtocolError::UnsupportedFieldValue(
                IP_V4_PROTO,
                "Internet Header Length",
                internet_header_length as usize,
            ));
        }
        if header_length as usize > data.len() {
            return Err(ProtocolError::InvalidMinimumLength(
                IP_V4_PROTO,
                header_length as usize,
                data.len(),
            ));
        }

        let type_of_service = data[1];
        let total_length = from_be_bytes!(u16, IP_V4_PROTO, &data[2..4]);
        let identification = from_be_bytes!(u16, IP_V4_PROTO, &data[4..6]);
        let fragment_offset = (from_be_bytes!(u16, IP_V4_PROTO, &data[6..8]) & 0x1FFF) * 8;
        let flags = (data[6] & 0xE0) >> 5;
        let time_to_live = data[8];
        let protocol = data[9];
        let header_checksum = from_be_bytes!(u16, IP_V4_PROTO, &data[10..12]);
        let source_address = Ipv4Addr::from(from_be_bytes!(u32, IP_V4_PROTO, &data[12..16]));
        let destination_address = Ipv4Addr::from(from_be_bytes!(u32, IP_V4_PROTO, &data[16..20]));

        Ok(IpV4Header {
            version: IpVersion::ipv4(),
            internet_header_length,
            type_of_service,
            total_length,
            identification,
            flags: Ipv4Flags::try_from(flags)?,
            fragment_offset,
            time_to_live,
            protocol,
            header_checksum,
            source_address,
            destination_address,
        })
    }
}
impl From<&IpV4Header> for Vec<u8> {
    fn from(header: &IpV4Header) -> Self {
        let mut buffer: Vec<u8> = Vec::new();
        buffer.reserve_exact(20);

        let flags_with_fragment_offset =
            header.fragment_offset | (u8::from(header.get_flags()) as u16) << 13;

        buffer.push((header.get_version().get_value() << 4) | (20u8 / 4));
        buffer.push(header.get_type_of_service());
        buffer.extend_from_slice(&header.total_length.to_be_bytes());
        buffer.extend_from_slice(&header.identification.to_be_bytes());
        buffer.extend_from_slice(&flags_with_fragment_offset.to_be_bytes());
        buffer.push(header.get_time_to_live());
        buffer.push(header.get_protocol());
        buffer.extend_from_slice(&header.header_checksum.to_be_bytes());
        buffer.extend_from_slice(&header.source_address.octets());
        buffer.extend_from_slice(&header.destination_address.octets());

        buffer
    }
}

#[cfg(test)]
mod tests {
    use rusty_router_proto_common::InternetChecksum;

    use super::*;

    #[test]
    fn parse_header() -> Result<()> {
        let data = [
            0x45, 12, 34, 56, 78, 90, 0xD1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11,
        ];
        let header = IpV4Header::try_from(&data[..])?;
        assert_eq!(InternetChecksum::checksum(&data[..])?, 57343);
        assert_eq!(header.get_version(), &IpVersion::ipv4());
        assert_eq!(header.get_internet_header_length(), 20 / 4);
        assert_eq!(header.get_total_length(), 8760);
        assert_eq!(header.get_identification(), 20058);
        assert_eq!(
            header.get_flags(),
            &Ipv4Flags::new(&[Ipv4Flag::DontFragment, Ipv4Flag::MoreFragments])
        );
        assert_eq!(header.get_type_of_service(), 12);
        assert_eq!(header.get_fragment_offset(), 35600);
        assert_eq!(header.get_time_to_live(), 127);
        assert_eq!(header.get_protocol(), 89);
        assert_eq!(header.get_header_checksum(), 0x888F);
        assert_eq!(header.get_source_address(), Ipv4Addr::from(127 << 24 | 1));
        assert_eq!(
            header.get_destination_address(),
            Ipv4Addr::from(8 << 24 | 9 << 16 | 10 << 8 | 11)
        );
        Ok(())
    }

    #[test]
    fn parse_small_header_sad() {
        let data = [
            0x45, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 1, 8, 9, 10, 11,
        ];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(
                e.to_string(),
                "[IPv4] Expecting data of at least 20 bytes but only 18 bytes are available"
                    .to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn parse_insufficient_header_data_sad() {
        let data = [
            0x46, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11,
        ];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(
                e.to_string(),
                "[IPv4] Expecting data of at least 24 bytes but only 20 bytes are available"
                    .to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn parse_invalid_header_length_sad() {
        let data = [
            0x44, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11,
        ];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(
                e.to_string(),
                "[IPv4] The value '4' for field 'Internet Header Length' is not supported"
                    .to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }

    #[test]
    fn parse_inversion_sad() {
        let data = [
            0x35, 12, 34, 56, 78, 90, 0xB1, 98, 127, 89, 0x88, 0x8f, 127, 0, 0, 1, 8, 9, 10, 11,
        ];

        if let Err(e) = IpV4Header::try_from(&data[..]) {
            assert_eq!(
                e.to_string(),
                "[IPv4] The value '3' for field 'Version' is not supported".to_string()
            );
        } else {
            assert!(false, "Result was expected to be an Err");
        }
    }
}
