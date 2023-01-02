#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Io Error: {0} - {1}")]
    Io(#[source] std::io::Error, String),
}