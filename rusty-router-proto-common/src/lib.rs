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

#[macro_export]
macro_rules! from_be_bytes {
    // ($ty:ty, $label:ident, $value:ident) => {{
    //     <$ty>::from_be_bytes($value.try_into().map_err(|_| ProtocolParseError::ConversionError($label, file!(), line!()))?)
    // }};
    ($ty:ty, $label:ident, $value:expr) => {{
        <$ty>::from_be_bytes($value.try_into().map_err(|_| ProtocolParseError::ConversionError($label, file!(), line!()))?)
    }};
}

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

#[cfg(test)]
mod tests {
    use std::panic;

    use super::*;

    #[test]
    pub fn test_conversion_ident() -> Result<(), ProtocolParseError> {
        let label = "Label";

        let value = vec![0xA1, 0xB3];
        let value_ref = &value[..];
        assert_eq!(from_be_bytes!(u16, label, value_ref), 0xA1B3);

        Ok(())
    }

    #[test]
    pub fn test_conversion_literal() -> Result<(), ProtocolParseError> {
        let label = "Label";
        assert_eq!(from_be_bytes!(u16, label, [0xA1, 0xB3][..]), 0xA1B3);

        Ok(())
    }

    #[test]
    pub fn test_conversion_failed_ident() -> Result<(), ProtocolParseError> {
        fn perform_test() -> Result<(), ProtocolParseError> {
            let label = "Label";
            let value = vec![0xA1];
            let value_ref = &value[..];
            from_be_bytes!(u16, label, value_ref);
            Ok(())
        }
        if let Err(ProtocolParseError::ConversionError(error_label, error_file, error_line)) = perform_test() {
            assert_eq!(error_label, "Label");
            assert_eq!(error_line, line!() - 5);
            assert_eq!(error_file, "rusty-router-proto-common/src/lib.rs");
        } else {
            panic!("Expected failure");
        }

        Ok(())
    }

    #[test]
    pub fn test_conversion_failed_literal() -> Result<(), ProtocolParseError> {
        fn perform_test() -> Result<(), ProtocolParseError> {
            let label = "Label";
            from_be_bytes!(u16, label, [0xA1][..]);
            Ok(())
        }
        if let Err(ProtocolParseError::ConversionError(error_label, error_file, error_line)) = perform_test() {
            assert_eq!(error_label, "Label");
            assert_eq!(error_line, line!() - 5);
            assert_eq!(error_file, "rusty-router-proto-common/src/lib.rs");
        } else {
            panic!("Expected failure");
        }

        Ok(())
    }

    #[test]
    pub fn test_checksum() -> Result<(), ProtocolParseError> {
        assert_eq!(InternetChecksum::checksum(&[])?, 65535);
        assert_eq!(InternetChecksum::checksum(&[0x80])?, 32767);
        assert_eq!(InternetChecksum::checksum(&[0x00, 0x01])?, 65534);
        assert_eq!(InternetChecksum::checksum(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])?, 0);
        assert_eq!(InternetChecksum::checksum(&[0x01, 0x03, 0x07, 0x0F, 0x11, 0x33, 0x77, 0xFF])?, 28347);
        Ok(())
    }

    #[test]
    pub fn test_checksum_update() -> Result<(), ProtocolParseError> {
        let mut checksum = InternetChecksum::new();
        checksum.add(&[0x01, 0x03, 0x07, 0x0F, 0x11, 0x33, 0x77, 0xFF, 0x10, 0x30, 0x77, 0xF0, 0x11, 0x33, 0x77, 0xFF])?;
        assert_eq!(checksum.calculate(), 23912);

        checksum.subtract(&[0x10, 0x30, 0x77, 0xFF])?;
        assert_eq!(checksum.calculate(), 58775);
        assert_eq!(InternetChecksum::checksum(&[0x01, 0x03, 0x07, 0x0F, 0x11, 0x33, 0x77, 0xF0, 0x11, 0x33, 0x77, 0xFF])?, 58775);

        Ok(())
    }
}
