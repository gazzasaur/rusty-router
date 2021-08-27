use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProtocolParseError {
    #[error("[{0}] Expecting data of at least {1} bytes but only {2} bytes are available")]
    InvalidMinimumLength(&'static str, usize, usize),
    #[error("[{0}] Expecting header to be {1} bytes but only {2} bytes are available")]
    InvalidHeaderLength(&'static str, usize, usize),
    #[error("[{0}] Expecting data to be {1} bytes but {2} bytes are available")]
    InvalidLength(&'static str, usize, usize),

    #[error("[{0}] Expecting data to be aligned with a word boundary but was {1} bytes long")]
    InvalidBitBoundary(&'static str, usize),

    #[error("[{0}] The value '{2}' for field '{1}' is not supported")]
    UnsupportedFieldValue(&'static str, &'static str, usize),

    #[error("[{0}] Failed to perform byte conversion {1}:{2}")]
    ConversionError(&'static str, &'static str, u32),
    #[error("[{proto}] Checksum mismatch.  Expected: {expected:#06x}, Actual: {actual:#06x}")]
    InvalidChecksum { proto: &'static str, expected: u16, actual: u16 },
}
