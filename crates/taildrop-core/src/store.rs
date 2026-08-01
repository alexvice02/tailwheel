//! Local persistence: settings, transfer history, and the staging area for
//! incoming files awaiting user confirmation. Everything lives under the
//! platform app-data directory as plain JSON — this is a single-user desktop
//! tool, not a multi-writer service, so a database would be pure overhead.
//!
//! Shared verbatim between the GUI (Tauri commands + background poller) and
//! `taildrop-cli`, so `send`/`confirm-drop` run from a terminal see exactly
//! the same history and pending queue as the window.

use crate::error::{Result, TaildropError};
use crate::models::{
    ConflictPolicy, HistoryRetention, PendingIncoming, Settings, TransferDirection, TransferRecord,
    TransferStatus,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SIDECAR_SUFFIX: &str = ".tdmeta.json";

/// Metadata sent as a small sidecar file alongside a real payload when both
/// sides happen to run tailwheel/taildrop-cli, so the receiver can show
/// "X sent you Y" instead of just "a file arrived". Tailscale's own LocalAPI
/// (`WaitingFile { Name, Size }`) exposes no sender identity at all, so
/// there's no way to get this from tailscaled itself — see the taildrop-core
/// design notes. Files from a plain `tailscale file cp` or the mobile/desktop
/// Tailscale apps simply won't have one, and that's handled gracefully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarMeta {
    pub file_name: String,
    pub size: u64,
    pub sender_hostname: String,
    pub sender_dns_name: String,
    pub sent_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PendingState {
    items: Vec<PendingIncoming>,
    unmatched_sidecars: Vec<SidecarMeta>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryState {
    records: Vec<TransferRecord>,
}

pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Uses the platform app-data directory (e.g. `~/.local/share/tailwheel`
    /// on Linux, `~/Library/Application Support/tailwheel` on macOS,
    /// `%APPDATA%\tailwheel` on Windows). Pass an explicit `root` (e.g. in
    /// tests) to override.
    pub fn new(root: Option<PathBuf>) -> Result<Self> {
        let root = match root {
            Some(r) => r,
            None => dirs::data_dir()
                .ok_or_else(|| TaildropError::Io("no app data directory for this platform".into()))?
                .join("tailwheel"),
        };
        fs::create_dir_all(&root).map_err(|e| TaildropError::Io(e.to_string()))?;
        fs::create_dir_all(root.join("staging")).map_err(|e| TaildropError::Io(e.to_string()))?;
        Ok(Self { root })
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join("staging")
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    fn history_path(&self) -> PathBuf {
        self.root.join("history.json")
    }

    fn pending_path(&self) -> PathBuf {
        self.root.join("pending.json")
    }

    fn read_json<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
        match fs::read_to_string(path) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(e) => Err(TaildropError::Io(e.to_string())),
        }
    }

    /// Write via a temp file + rename so a crash mid-write can't corrupt the
    /// previous, valid state.
    fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(value)?;
        fs::write(&tmp, text).map_err(|e| TaildropError::Io(e.to_string()))?;
        fs::rename(&tmp, path).map_err(|e| TaildropError::Io(e.to_string()))?;
        Ok(())
    }

    // --- settings ---------------------------------------------------------

    pub fn load_settings(&self) -> Result<Settings> {
        match fs::read_to_string(self.settings_path()) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(TaildropError::Io(e.to_string())),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        Self::write_json(&self.settings_path(), settings)
    }

    // --- history ------------------------------------------------------------

    pub fn list_history(&self) -> Result<Vec<TransferRecord>> {
        let state: HistoryState = Self::read_json(&self.history_path())?;
        Ok(state.records)
    }

    pub fn append_history(&self, record: TransferRecord) -> Result<()> {
        let mut state: HistoryState = Self::read_json(&self.history_path())?;
        state.records.insert(0, record);
        Self::write_json(&self.history_path(), &state)
    }

    pub fn clear_history(&self) -> Result<()> {
        Self::write_json(&self.history_path(), &HistoryState::default())
    }

    // --- pending incoming ---------------------------------------------------

    fn load_pending_state(&self) -> Result<PendingState> {
        Self::read_json(&self.pending_path())
    }

    fn save_pending_state(&self, state: &PendingState) -> Result<()> {
        Self::write_json(&self.pending_path(), state)
    }

    pub fn list_pending(&self) -> Result<Vec<PendingIncoming>> {
        Ok(self.load_pending_state()?.items)
    }

    pub fn is_sidecar_file(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(SIDECAR_SUFFIX))
    }

    pub fn sidecar_file_name(id: &uuid::Uuid) -> String {
        format!("{id}{SIDECAR_SUFFIX}")
    }

    /// Reconcile freshly-drained staging files (from `tailscale::drain_inbox`)
    /// into the pending queue: sidecar metadata files are parsed, matched
    /// against a real file by name (in either order of arrival), and
    /// consumed; everything else becomes a `PendingIncoming` awaiting
    /// accept/reject. Returns the newly-created pending entries.
    pub fn ingest_drained_files(&self, new_paths: Vec<PathBuf>) -> Result<Vec<PendingIncoming>> {
        let mut state = self.load_pending_state()?;
        let mut real_files = Vec::new();

        for path in new_paths {
            if Self::is_sidecar_file(&path) {
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<SidecarMeta>(&text) {
                        state.unmatched_sidecars.push(meta);
                    }
                }
                let _ = fs::remove_file(&path);
            } else {
                real_files.push(path);
            }
        }

        let mut newly_added = Vec::new();
        for path in real_files {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let matched = state
                .unmatched_sidecars
                .iter()
                .position(|m| m.file_name == file_name)
                .map(|i| state.unmatched_sidecars.remove(i));

            let (sender_hostname, sender_dns_name) = match matched {
                Some(m) => (Some(m.sender_hostname), Some(m.sender_dns_name)),
                None => (None, None),
            };

            let item = PendingIncoming {
                id: uuid::Uuid::new_v4().to_string(),
                file_name,
                staged_path: path.to_string_lossy().into_owned(),
                size,
                sender_hostname,
                sender_dns_name,
                received_at: chrono::Utc::now().to_rfc3339(),
            };
            state.items.push(item.clone());
            newly_added.push(item);
        }

        // A sidecar can legitimately arrive after its file (ordering over the
        // wire isn't guaranteed), so give already-pending, sender-less items
        // another chance to match on every ingest too.
        for item in state.items.iter_mut() {
            if item.sender_hostname.is_none() {
                if let Some(i) = state
                    .unmatched_sidecars
                    .iter()
                    .position(|m| m.file_name == item.file_name)
                {
                    let m = state.unmatched_sidecars.remove(i);
                    item.sender_hostname = Some(m.sender_hostname);
                    item.sender_dns_name = Some(m.sender_dns_name);
                }
            }
        }

        self.save_pending_state(&state)?;
        Ok(newly_added)
    }

    /// Move a pending file's bytes into the configured save directory and
    /// record it in history. This is the only path by which a file leaves
    /// staging successfully — never a raw copy elsewhere.
    pub fn accept_pending(&self, id: &str, settings: &Settings) -> Result<TransferRecord> {
        let mut state = self.load_pending_state()?;
        let idx = state
            .items
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| TaildropError::NotFound(format!("pending item {id}")))?;
        let item = state.items.remove(idx);
        self.save_pending_state(&state)?;

        let staged = PathBuf::from(&item.staged_path);
        let dest_dir = PathBuf::from(&settings.save_dir);
        fs::create_dir_all(&dest_dir).map_err(|e| TaildropError::Io(e.to_string()))?;
        let dest_path = resolve_destination(&dest_dir.join(&item.file_name), settings.conflict_policy);

        move_file(&staged, &dest_path)?;

        let record = TransferRecord {
            id: item.id,
            direction: TransferDirection::Received,
            peer_hostname: item.sender_hostname.clone().unwrap_or_else(|| "unknown".into()),
            peer_dns_name: item.sender_dns_name.clone(),
            file_name: item.file_name,
            size: item.size,
            timestamp: chrono::Utc::now().to_rfc3339(),
            status: TransferStatus::Completed,
            saved_path: Some(dest_path.to_string_lossy().into_owned()),
            error: None,
            batch_id: None,
        };
        self.append_history(record.clone())?;
        Ok(record)
    }

    pub fn reject_pending(&self, id: &str) -> Result<TransferRecord> {
        let mut state = self.load_pending_state()?;
        let idx = state
            .items
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| TaildropError::NotFound(format!("pending item {id}")))?;
        let item = state.items.remove(idx);
        self.save_pending_state(&state)?;

        let staged = PathBuf::from(&item.staged_path);
        let _ = fs::remove_file(&staged);

        let record = TransferRecord {
            id: item.id,
            direction: TransferDirection::Received,
            peer_hostname: item.sender_hostname.clone().unwrap_or_else(|| "unknown".into()),
            peer_dns_name: item.sender_dns_name.clone(),
            file_name: item.file_name,
            size: item.size,
            timestamp: chrono::Utc::now().to_rfc3339(),
            status: TransferStatus::Rejected,
            saved_path: None,
            error: None,
            batch_id: None,
        };
        self.append_history(record.clone())?;
        Ok(record)
    }

    /// Delete history entries older than `retention` allows, as of `now`.
    /// A no-op for `HistoryRetention::Never`. Called opportunistically (on
    /// poll ticks and right after a settings change) rather than on a
    /// separate scheduler, since this is a single-user desktop app and the
    /// history file is small enough that a full rewrite is cheap.
    pub fn prune_history(&self, retention: HistoryRetention, now: chrono::DateTime<chrono::Utc>) -> Result<()> {
        let Some(max_age_days) = retention.max_age_days() else {
            return Ok(());
        };
        let cutoff = now - chrono::Duration::days(max_age_days);

        let mut state: HistoryState = Self::read_json(&self.history_path())?;
        let before = state.records.len();
        state.records.retain(|r| {
            match chrono::DateTime::parse_from_rfc3339(&r.timestamp) {
                Ok(t) => t.with_timezone(&chrono::Utc) >= cutoff,
                // Keep anything we can't parse rather than silently losing it.
                Err(_) => true,
            }
        });

        if state.records.len() != before {
            Self::write_json(&self.history_path(), &state)?;
        }
        Ok(())
    }
}

/// Pick a final destination path, avoiding clobbering an existing file
/// unless the policy explicitly allows it.
fn resolve_destination(preferred: &Path, policy: ConflictPolicy) -> PathBuf {
    if !preferred.exists() || policy == ConflictPolicy::Overwrite {
        return preferred.to_path_buf();
    }
    let stem = preferred
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = preferred.extension().map(|e| e.to_string_lossy().into_owned());
    let parent = preferred.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    for n in 1..10_000 {
        let candidate_name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = parent.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    preferred.to_path_buf()
}

/// Rename when possible (same filesystem, the common case since both staging
/// and the default save dir live under the same app-data/home volume), and
/// fall back to copy+delete for cross-device moves (e.g. a save dir on a
/// different drive).
fn move_file(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(|e| TaildropError::Io(e.to_string()))?;
    fs::remove_file(from).map_err(|e| TaildropError::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Isolated scratch dir per test so runs can't interfere with each other
    /// or a real `~/.local/share/tailwheel`.
    fn test_store() -> (Store, PathBuf) {
        let root = std::env::temp_dir().join(format!("taildrop-core-test-{}", uuid::Uuid::new_v4()));
        (Store::new(Some(root.clone())).unwrap(), root)
    }

    fn cleanup(root: PathBuf) {
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn settings_roundtrip_and_defaults() {
        let (store, root) = test_store();
        let defaults = store.load_settings().unwrap();
        assert!(!defaults.save_dir.is_empty());
        assert_eq!(defaults.conflict_policy, ConflictPolicy::Rename);

        let mut updated = defaults;
        updated.auto_accept = true;
        updated.poll_interval_secs = 7;
        store.save_settings(&updated).unwrap();
        let reloaded = store.load_settings().unwrap();
        assert!(reloaded.auto_accept);
        assert_eq!(reloaded.poll_interval_secs, 7);
        cleanup(root);
    }

    #[test]
    fn ingest_matches_sidecar_arriving_before_file() {
        let (store, root) = test_store();
        let staging = store.staging_dir();

        let sidecar = SidecarMeta {
            file_name: "photo.jpg".into(),
            size: 42,
            sender_hostname: "alices-laptop".into(),
            sender_dns_name: "alices-laptop.tailnet.ts.net".into(),
            sent_at: chrono::Utc::now().to_rfc3339(),
        };
        let sidecar_id = uuid::Uuid::new_v4();
        let sidecar_path = staging.join(Store::sidecar_file_name(&sidecar_id));
        fs::write(&sidecar_path, serde_json::to_string(&sidecar).unwrap()).unwrap();

        let file_path = staging.join("photo.jpg");
        fs::write(&file_path, b"fake jpg bytes").unwrap();

        // sidecar and file arrive in the same batch, sidecar first
        let added = store
            .ingest_drained_files(vec![sidecar_path, file_path])
            .unwrap();

        assert_eq!(added.len(), 1);
        assert_eq!(added[0].file_name, "photo.jpg");
        assert_eq!(added[0].sender_hostname.as_deref(), Some("alices-laptop"));
        // sidecar must be consumed, not left behind as its own pending item
        assert_eq!(store.list_pending().unwrap().len(), 1);
        cleanup(root);
    }

    #[test]
    fn ingest_matches_sidecar_arriving_after_file() {
        let (store, root) = test_store();
        let staging = store.staging_dir();

        let file_path = staging.join("report.pdf");
        fs::write(&file_path, b"fake pdf bytes").unwrap();
        let added = store.ingest_drained_files(vec![file_path]).unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].sender_hostname, None);

        let sidecar = SidecarMeta {
            file_name: "report.pdf".into(),
            size: 14,
            sender_hostname: "bobs-phone".into(),
            sender_dns_name: "bobs-phone.tailnet.ts.net".into(),
            sent_at: chrono::Utc::now().to_rfc3339(),
        };
        let sidecar_path = staging.join(Store::sidecar_file_name(&uuid::Uuid::new_v4()));
        fs::write(&sidecar_path, serde_json::to_string(&sidecar).unwrap()).unwrap();
        store.ingest_drained_files(vec![sidecar_path]).unwrap();

        let pending = store.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sender_hostname.as_deref(), Some("bobs-phone"));
        cleanup(root);
    }

    #[test]
    fn accept_moves_file_and_records_history() {
        let (store, root) = test_store();
        let staging = store.staging_dir();
        let save_dir = root.join("save");

        let file_path = staging.join("doc.txt");
        fs::write(&file_path, b"hello").unwrap();
        let added = store.ingest_drained_files(vec![file_path]).unwrap();
        let id = added[0].id.clone();

        let mut settings = store.load_settings().unwrap();
        settings.save_dir = save_dir.to_string_lossy().into_owned();
        settings.conflict_policy = ConflictPolicy::Rename;

        let record = store.accept_pending(&id, &settings).unwrap();
        assert_eq!(record.status, TransferStatus::Completed);
        let saved_path = PathBuf::from(record.saved_path.unwrap());
        assert!(saved_path.exists());
        assert!(!staging.join("doc.txt").exists());
        assert!(store.list_pending().unwrap().is_empty());

        let history = store.list_history().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].direction, TransferDirection::Received);
        cleanup(root);
    }

    #[test]
    fn accept_renames_on_name_collision() {
        let (store, root) = test_store();
        let staging = store.staging_dir();
        let save_dir = root.join("save");
        fs::create_dir_all(&save_dir).unwrap();
        fs::write(save_dir.join("doc.txt"), b"existing").unwrap();

        let file_path = staging.join("doc.txt");
        fs::write(&file_path, b"new content").unwrap();
        let added = store.ingest_drained_files(vec![file_path]).unwrap();

        let mut settings = store.load_settings().unwrap();
        settings.save_dir = save_dir.to_string_lossy().into_owned();
        settings.conflict_policy = ConflictPolicy::Rename;

        let record = store.accept_pending(&added[0].id, &settings).unwrap();
        let saved_path = PathBuf::from(record.saved_path.unwrap());
        assert_ne!(saved_path, save_dir.join("doc.txt"));
        assert_eq!(fs::read_to_string(&save_dir.join("doc.txt")).unwrap(), "existing");
        assert_eq!(fs::read_to_string(&saved_path).unwrap(), "new content");
        cleanup(root);
    }

    #[test]
    fn reject_deletes_staged_file_and_records_history() {
        let (store, root) = test_store();
        let staging = store.staging_dir();

        let file_path = staging.join("unwanted.bin");
        fs::write(&file_path, b"nope").unwrap();
        let added = store.ingest_drained_files(vec![file_path.clone()]).unwrap();

        let record = store.reject_pending(&added[0].id).unwrap();
        assert_eq!(record.status, TransferStatus::Rejected);
        assert!(!file_path.exists());
        assert!(store.list_pending().unwrap().is_empty());
        cleanup(root);
    }

    fn fake_record(id: &str, timestamp: String) -> TransferRecord {
        TransferRecord {
            id: id.to_string(),
            direction: TransferDirection::Sent,
            peer_hostname: "somewhere".into(),
            peer_dns_name: None,
            file_name: "f.txt".into(),
            size: 1,
            timestamp,
            status: TransferStatus::Completed,
            saved_path: None,
            error: None,
            batch_id: None,
        }
    }

    #[test]
    fn prune_history_removes_only_entries_older_than_retention() {
        let (store, root) = test_store();
        let now = chrono::Utc::now();

        store.append_history(fake_record("old", (now - chrono::Duration::days(10)).to_rfc3339())).unwrap();
        store.append_history(fake_record("recent", (now - chrono::Duration::hours(1)).to_rfc3339())).unwrap();

        store.prune_history(HistoryRetention::Weekly, now).unwrap();

        let remaining = store.list_history().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "recent");
        cleanup(root);
    }

    #[test]
    fn prune_history_never_keeps_everything() {
        let (store, root) = test_store();
        let now = chrono::Utc::now();

        store.append_history(fake_record("ancient", (now - chrono::Duration::days(9999)).to_rfc3339())).unwrap();

        store.prune_history(HistoryRetention::Never, now).unwrap();

        assert_eq!(store.list_history().unwrap().len(), 1);
        cleanup(root);
    }

    #[test]
    fn clear_history_empties_the_log() {
        let (store, root) = test_store();
        let now = chrono::Utc::now();

        store.append_history(fake_record("one", now.to_rfc3339())).unwrap();
        store.append_history(fake_record("two", now.to_rfc3339())).unwrap();
        assert_eq!(store.list_history().unwrap().len(), 2);

        store.clear_history().unwrap();

        assert!(store.list_history().unwrap().is_empty());
        cleanup(root);
    }

    #[test]
    fn resolve_destination_overwrite_vs_rename() {
        let root = std::env::temp_dir().join(format!("taildrop-resolve-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let existing = root.join("a.txt");
        fs::write(&existing, b"x").unwrap();

        assert_eq!(resolve_destination(&existing, ConflictPolicy::Overwrite), existing);
        let renamed = resolve_destination(&existing, ConflictPolicy::Rename);
        assert_ne!(renamed, existing);
        assert_eq!(renamed, root.join("a (1).txt"));

        let fresh = root.join("b.txt");
        assert_eq!(resolve_destination(&fresh, ConflictPolicy::Rename), fresh);
        cleanup(root);
    }
}
