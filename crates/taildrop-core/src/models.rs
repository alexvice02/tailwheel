use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub hostname: String,
    pub dns_name: String,
    pub os: String,
    pub tailscale_ips: Vec<String>,
    pub online: bool,
    pub last_seen: Option<String>,
    pub is_self: bool,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailnetStatus {
    pub self_peer: Peer,
    pub peers: Vec<Peer>,
}

/// A valid `tailscale file cp` target, as reported by `--targets`.
/// More lightweight than a full `Peer` and reflects exactly who you're
/// currently allowed to send files to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpTarget {
    pub ip: String,
    pub name: String,
    pub offline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitingFile {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    Skip,
    Overwrite,
    Rename,
}

impl ConflictPolicy {
    pub fn as_flag(&self) -> &'static str {
        match self {
            ConflictPolicy::Skip => "skip",
            ConflictPolicy::Overwrite => "overwrite",
            ConflictPolicy::Rename => "rename",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    Pending,
    Completed,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: String,
    pub direction: TransferDirection,
    pub peer_hostname: String,
    pub peer_dns_name: Option<String>,
    pub file_name: String,
    pub size: u64,
    pub timestamp: String,
    pub status: TransferStatus,
    pub saved_path: Option<String>,
    pub error: Option<String>,
}

/// A file that has been pulled out of the Tailscale daemon's inbox into our
/// own staging directory, and is waiting for the user to accept or reject it.
/// Staging (rather than writing straight to the configured save directory)
/// is what makes "reject" possible at all: once `tailscale file get` hands us
/// the bytes there is no way to put them back in the daemon's inbox, so we
/// keep them in a holding area under our control until the user decides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingIncoming {
    pub id: String,
    pub file_name: String,
    pub staged_path: String,
    pub size: u64,
    pub sender_hostname: Option<String>,
    pub sender_dns_name: Option<String>,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStat {
    pub peer: Peer,
    pub sent_count: u64,
    pub sent_bytes: u64,
    pub received_count: u64,
    pub received_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStats {
    pub devices: Vec<DeviceStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub save_dir: String,
    pub auto_accept: bool,
    pub conflict_policy: ConflictPolicy,
    pub poll_interval_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        let save_dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        Self {
            save_dir,
            auto_accept: false,
            conflict_policy: ConflictPolicy::Rename,
            poll_interval_secs: 3,
        }
    }
}
