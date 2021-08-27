use std::convert::TryInto;

mod error;
pub use error::*;

#[cfg(target_endian = "big")]
const NATIVE_NUMBER_FROM_BYTES: fn([u8; 8]) -> u64 = u64::from_be_bytes;
#[cfg(target_endian = "big")]
const NATIVE_NUMBER_FROM_NETWORK: bool = false;

#[cfg(target_endian = "little")]
const NATIVE_NUMBER_FROM_BYTES: fn([u8; 8]) -> u64 = u64::from_le_bytes;
#[cfg(target_endian = "little")]
const NATIVE_NUMBER_FROM_NETWORK: bool = true;

pub struct InternetChecksum {
    accumulator: u128,
}
impl InternetChecksum {
    pub fn new() -> InternetChecksum {
        InternetChecksum { accumulator: 0 }
    }

    pub fn add(&mut self, data: &[u8]) -> Result<(), ProtocolParseError> {
        self.accumulator += Self::calculate_partial_result(data)?;
        Ok(())
    }

    pub fn subtract(&mut self, data: &[u8]) -> Result<(), ProtocolParseError> {
        self.accumulator -= Self::calculate_partial_result(data)?;
        Ok(())
    }

    pub fn calculate(&self) -> u16 {
        Self::calculate_final_result(self.accumulator)
    }

    pub fn checksum(data: &[u8]) -> Result<u16, ProtocolParseError> {
        Ok(Self::calculate_final_result(Self::calculate_partial_result(data)?))
    }

    fn calculate_partial_result(data: &[u8]) -> Result<u128, ProtocolParseError> {
        let mut partial_accumulator: u128 = 0;
        for i in 0..(data.len()/8) {
            // This should not be reachable.  But in the off chance it is, don't panic.  Allow the application to choose the next steps.
            let byte_data = data[i*8..(i*8 + 8)].try_into().map_err(|_| ProtocolParseError::ConversionError("Checksum", file!(), line!()))?;
            partial_accumulator += NATIVE_NUMBER_FROM_BYTES(byte_data) as u128;
        }
        if NATIVE_NUMBER_FROM_NETWORK {
            partial_accumulator = u128::from_be(partial_accumulator);
        }
        for i in 0..(data.len()%8) {
            partial_accumulator += (data[8*(data.len()/8) + i] as u128) << (8 * (7 - i));
        }
        Ok(partial_accumulator)
    }

    fn calculate_final_result(accumulator: u128) -> u16 {
        let mut checksum_aggregator = accumulator;
        // Fold the accumulator in halves twice to account for carry.
        checksum_aggregator = (checksum_aggregator & 0x0000000000000000FFFFFFFFFFFFFFFF) + (checksum_aggregator >> 64);
        checksum_aggregator = (checksum_aggregator & 0x0000000000000000FFFFFFFFFFFFFFFF) + (checksum_aggregator >> 64);
        checksum_aggregator = (checksum_aggregator & 0x000000000000000000000000FFFFFFFF) + (checksum_aggregator >> 32);
        checksum_aggregator = (checksum_aggregator & 0x000000000000000000000000FFFFFFFF) + (checksum_aggregator >> 32);
        checksum_aggregator = (checksum_aggregator & 0x0000000000000000000000000000FFFF) + (checksum_aggregator >> 16);
        checksum_aggregator = (checksum_aggregator & 0x0000000000000000000000000000FFFF) + (checksum_aggregator >> 16);
        checksum_aggregator ^= 0x0000FFFF;

        return checksum_aggregator as u16;
    }
}

