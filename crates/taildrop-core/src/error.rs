use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaildropError {
    #[error("failed to launch tailscale CLI: {0}")]
    Spawn(std::io::Error),

    #[error("{0}")]
    Command(String),

    #[error("failed to parse tailscale output: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, TaildropError>;
