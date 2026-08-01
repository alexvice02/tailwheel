use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaildropError {
    #[error("failed to launch tailscale CLI: {0}")]
    Spawn(std::io::Error),

    #[error("{0}")]
    Command(String),

    /// Distinguished from `Command` so callers (the GUI in particular) can
    /// tell "tailscale rejected this" apart from "we gave up waiting" and
    /// show different UI — e.g. "device is offline" rather than a generic
    /// failure. The message is prefixed with `timeout:` so it survives the
    /// `TaildropError -> String` conversion at the Tauri command boundary
    /// (Tauri needs a serializable error, so structured variants don't
    /// reach the frontend as anything but a plain string).
    #[error("timeout: {0}")]
    Timeout(String),

    #[error("failed to parse tailscale output: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, TaildropError>;
