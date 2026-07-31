pub mod error;
pub mod format;
pub mod models;
pub mod store;
pub mod tailscale;

use error::Result;
use models::{
    ConflictPolicy, DeviceStats, PendingIncoming, Settings, TransferDirection, TransferRecord,
    TransferStatus,
};
use std::path::Path;
use store::{SidecarMeta, Store};

/// Send `file` to `target`, first pushing a small `.tdmeta.json` sidecar so
/// that a receiver also running taildrop-gui/taildrop-cli can attribute the
/// sender. Records the attempt in history either way (Completed on success,
/// Failed with the error message otherwise) so the CLI and GUI history views
/// agree on what happened.
pub fn send_with_attribution(
    store: &Store,
    self_hostname: &str,
    self_dns_name: &str,
    target: &str,
    file: &Path,
) -> Result<TransferRecord> {
    let file_name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);

    let sidecar_id = uuid::Uuid::new_v4();
    let sidecar = SidecarMeta {
        file_name: file_name.clone(),
        size,
        sender_hostname: self_hostname.to_string(),
        sender_dns_name: self_dns_name.to_string(),
        sent_at: chrono::Utc::now().to_rfc3339(),
    };
    let sidecar_path = std::env::temp_dir().join(Store::sidecar_file_name(&sidecar_id));
    let sidecar_name = Store::sidecar_file_name(&sidecar_id);

    let result = (|| -> Result<()> {
        let text = serde_json::to_string(&sidecar)?;
        std::fs::write(&sidecar_path, text).map_err(|e| error::TaildropError::Io(e.to_string()))?;
        // Best-effort: a receiver not running our tooling just gets an extra
        // small file in their inbox, which is harmless.
        let _ = tailscale::send_file(target, &sidecar_path, Some(&sidecar_name));
        tailscale::send_file(target, file, None)
    })();

    let _ = std::fs::remove_file(&sidecar_path);

    let record = TransferRecord {
        id: uuid::Uuid::new_v4().to_string(),
        direction: TransferDirection::Sent,
        peer_hostname: target.to_string(),
        peer_dns_name: None,
        file_name,
        size,
        timestamp: chrono::Utc::now().to_rfc3339(),
        status: match &result {
            Ok(()) => TransferStatus::Completed,
            Err(_) => TransferStatus::Failed,
        },
        saved_path: None,
        error: result.as_ref().err().map(|e| e.to_string()),
    };
    store.append_history(record.clone())?;
    result.map(|_| record)
}

/// One pass of: drain whatever's sitting in the Tailscale inbox into our
/// staging area, then reconcile it against sidecars into pending entries.
/// Meant to be called on a timer by the GUI's background poller and by
/// `taildrop-cli confirm-drop --list` (via `refresh`) alike.
pub fn poll_inbox_once(store: &Store, conflict: ConflictPolicy) -> Result<Vec<PendingIncoming>> {
    let drained = tailscale::drain_inbox(&store.staging_dir(), conflict)?;
    if drained.is_empty() {
        return Ok(Vec::new());
    }
    store.ingest_drained_files(drained)
}

/// Tailnet device list enriched with this app's own send/receive counts per
/// peer, for the device/network statistics view.
pub fn device_stats(store: &Store) -> Result<DeviceStats> {
    let status = tailscale::status()?;
    let history = store.list_history()?;

    let mut per_peer: std::collections::HashMap<String, (u64, u64, u64, u64)> =
        std::collections::HashMap::new();
    for record in &history {
        let entry = per_peer.entry(record.peer_hostname.clone()).or_default();
        match (record.direction, record.status) {
            (TransferDirection::Sent, TransferStatus::Completed) => {
                entry.0 += 1;
                entry.1 += record.size;
            }
            (TransferDirection::Received, TransferStatus::Completed) => {
                entry.2 += 1;
                entry.3 += record.size;
            }
            _ => {}
        }
    }

    let devices = std::iter::once(&status.self_peer)
        .chain(status.peers.iter())
        .map(|peer| {
            let (sent_count, sent_bytes, received_count, received_bytes) =
                per_peer.get(&peer.hostname).copied().unwrap_or_default();
            models::DeviceStat {
                peer: peer.clone(),
                sent_count,
                sent_bytes,
                received_count,
                received_bytes,
            }
        })
        .collect();

    Ok(DeviceStats { devices })
}

pub fn load_or_init_settings(store: &Store) -> Result<Settings> {
    store.load_settings()
}
