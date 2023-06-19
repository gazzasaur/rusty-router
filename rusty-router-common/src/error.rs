#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Illegal State Error: {0}")]
    IllegalState(String),

    #[error("Communication Error: {0}")]
    Communication(String),

    #[error("Io Error: {0} - {1}")]
    Io(#[source] std::io::Error, String),

    #[error("System Error: {0}")]
    System(String),

    #[error("Protocol Error: {0}")]
    Protocol(String),
}
