pub use crate::error::ProtocolError;

pub type Result<T> = core::result::Result<T, ProtocolError>;
