pub mod error;
pub mod format;
pub mod models;
pub mod store;
pub mod tailscale;

use error::{Result, TaildropError};
use models::{
    ConflictPolicy, DeviceStats, PendingIncoming, Settings, TransferDirection, TransferRecord,
    TransferStatus,
};
use std::path::{Path, PathBuf};
use store::{SidecarMeta, Store};

/// Send `file` to `target`, first pushing a small `.tdmeta.json` sidecar so
/// that a receiver also running tailwheel/taildrop-cli can attribute the
/// sender. Records the attempt in history either way (Completed on success,
/// Failed with the error message otherwise) so the CLI and GUI history views
/// agree on what happened. `batch_id` should be the same value for every file
/// that was queued up in one send action, so the UI can group them; pass
/// `None` for a lone file.
pub fn send_with_attribution(
    store: &Store,
    self_hostname: &str,
    self_dns_name: &str,
    target: &str,
    file: &Path,
    batch_id: Option<&str>,
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
        // small file in their inbox, which is harmless — except a timeout,
        // which means the target is unreachable and the real payload below
        // would just time out too. Bail out now instead of making the
        // caller wait through two timeouts back to back for one failure.
        match tailscale::send_file(target, &sidecar_path, Some(&sidecar_name)) {
            Err(TaildropError::Timeout(msg)) => return Err(TaildropError::Timeout(msg)),
            _ => {}
        }
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
        batch_id: batch_id.map(|s| s.to_string()),
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

/// The no-subprocess counterpart to `poll_inbox_once`, for when a long-lived
/// `tailscale file get --loop` receiver (see `tailscale::spawn_receiver`) is
/// already draining the inbox into staging: this just picks up whatever
/// landed there since last time. A directory listing costs microseconds, so
/// unlike the drain-based poll it can run on a short timer without the app
/// spawning a process every few seconds all day.
pub fn poll_staging_once(store: &Store) -> Result<Vec<PendingIncoming>> {
    let new_files = store.unclaimed_staging_files()?;
    if new_files.is_empty() {
        return Ok(Vec::new());
    }
    store.ingest_drained_files(new_files)
}

/// Tailnet device list enriched with this app's own send/receive counts per
/// peer, for the device/network statistics view. `fresh` bypasses
/// `tailscale::status`'s cache; pass it only for user-initiated refreshes.
pub fn device_stats(store: &Store, fresh: bool) -> Result<DeviceStats> {
    let status = tailscale::status(fresh)?;
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

/// Expand file-picker output into a flat list of sendable files. Taildrop
/// (like Tailscale's own client) only ever transfers individual files, so
/// when the user picks a directory via the "Browse folder" option we walk it
/// and queue every file underneath instead of trying to send the directory
/// itself. Paths that are already files pass through unchanged; symlinks and
/// other non-regular entries are silently skipped rather than erroring, so
/// one odd entry doesn't block the rest of the picked files.
pub fn expand_send_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        collect_files(path, &mut files)?;
    }
    Ok(files)
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    if metadata.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| error::TaildropError::Io(e.to_string()))?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            collect_files(&entry.path(), out)?;
        }
    } else if metadata.is_file() {
        out.push(path.to_path_buf());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_send_paths_walks_directories_and_passes_through_files() {
        let tmp = std::env::temp_dir().join(format!("tailwheel-expand-test-{}", uuid::Uuid::new_v4()));
        let nested = tmp.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(tmp.join("a.txt"), b"a").unwrap();
        std::fs::write(nested.join("b.txt"), b"b").unwrap();

        let result = expand_send_paths(&[tmp.clone()]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&tmp.join("a.txt")));
        assert!(result.contains(&nested.join("b.txt")));

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    /// The receive path the GUI actually runs: a long-lived child dropping
    /// files into staging on its own schedule, and a poll that notices them
    /// without shelling out. Uses a stand-in for `tailscale` because Taildrop
    /// refuses to send to the machine it's running on, so there's no way to
    /// exercise a real `file get --loop` on a single host.
    ///
    /// Windows batch files aren't shell scripts and this covers our half of
    /// the mechanism (supervision + ingest), not the CLI's, so Unix is enough.
    #[test]
    #[cfg(unix)]
    fn receiver_child_drops_files_into_staging_and_the_poll_ingests_them() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("tailwheel-receiver-{}", uuid::Uuid::new_v4()));
        let store = Store::new(Some(root.clone())).unwrap();

        // Stands in for `tailscale file get --loop <dir>`: drops a file into
        // the destination, then keeps running the way the real one does.
        let fake = root.join("fake-tailscale.sh");
        {
            let mut f = std::fs::File::create(&fake).unwrap();
            writeln!(f, "#!/bin/sh\nsleep 0.2\necho payload > \"$5/from-peer.bin\"\nsleep 30\n").unwrap();
        }
        let mut perms = std::fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake, perms).unwrap();

        let mut child =
            tailscale::spawn_receiver_bin(&fake, &store.staging_dir(), ConflictPolicy::Rename)
                .unwrap();

        let staged = store.staging_dir().join("from-peer.bin");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !staged.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(staged.exists(), "receiver child never wrote the file");
        assert!(
            child.try_wait().unwrap().is_none(),
            "receiver child should still be running, not exit after one file"
        );

        // Backdate past the settle window instead of sleeping through it.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&staged)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();

        let ingested = poll_staging_once(&store).unwrap();
        assert_eq!(ingested.len(), 1);
        assert_eq!(ingested[0].file_name, "from-peer.bin");
        assert!(
            poll_staging_once(&store).unwrap().is_empty(),
            "a second poll must not re-ingest the same file"
        );

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn expand_send_paths_passes_plain_files_through() {
        let tmp = std::env::temp_dir().join(format!("tailwheel-expand-file-{}", uuid::Uuid::new_v4()));
        std::fs::write(&tmp, b"x").unwrap();

        let result = expand_send_paths(&[tmp.clone()]).unwrap();

        assert_eq!(result, vec![tmp.clone()]);
        std::fs::remove_file(&tmp).unwrap();
    }
}
