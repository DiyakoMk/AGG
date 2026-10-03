use thiserror::Error;

#[derive(Debug, Error)]
pub enum AggError {
    #[error("{0}")]
    Config(String),
    #[error("tunnel: {0}")]
    Tunnel(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl AggError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
}
