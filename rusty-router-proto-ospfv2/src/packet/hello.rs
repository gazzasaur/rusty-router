use std::convert::TryFrom;
use super::header::{PROTOCOL, OspfHeader};
use rusty_router_proto_common::error::ProtocolError;

#[derive(Debug)]
pub struct OspfHelloPacket {
    // header: OspfHeader,
    // network_mask: u32,
    // hello_interval: u16,
    // options: u8,
    // router_priority: u8,
    // router_dead_interval: u16,
    // designated_router: u32,
    // backup_designated_router: u32,
    // neighbors: Vec<u32>,
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
        

        // Ok(OspfHelloPacket {
        //     header,
        //     network_mask: u32::from_be_bytes(data[4..8].try_into().map_err(|_| OspfParseError::ConversionError(file!(), line!()))?),
        // });
        Err(ProtocolError::UnsupportedFieldValue(PROTOCOL, "TODO", 0))
    }
}
