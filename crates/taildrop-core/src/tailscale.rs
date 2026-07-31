//! Thin wrapper around the `tailscale` CLI. We shell out rather than talk to
//! tailscaled's LocalAPI socket directly: the CLI already knows the right
//! socket/pipe path and permission model on every platform, and it's the
//! only interface Tailscale documents as stable. `tailscale debug localapi`
//! *can* reach the raw LocalAPI (see `peek_waiting_files`), but that surface
//! is explicitly unstable and 404s on some builds, so it's used only as a
//! best-effort enhancement, never as the sole mechanism for anything.

use crate::error::{Result, TaildropError};
use crate::models::{ConflictPolicy, CpTarget, Peer, TailnetStatus, WaitingFile};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> PathBuf {
    if let Ok(p) = std::env::var("TAILSCALE_BIN") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "macos")]
    {
        let candidate = PathBuf::from("/Applications/Tailscale.app/Contents/MacOS/Tailscale");
        if candidate.exists() {
            return candidate;
        }
    }
    #[cfg(windows)]
    {
        let candidate = PathBuf::from(r"C:\Program Files\Tailscale\tailscale.exe");
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("tailscale")
}

fn run(args: &[&str]) -> Result<String> {
    let output = Command::new(binary())
        .args(args)
        .output()
        .map_err(TaildropError::Spawn)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(TaildropError::Command(if stderr.is_empty() {
            format!("`tailscale {}` exited with {}", args.join(" "), output.status)
        } else {
            stderr
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// --- status / peers -------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct RawPeer {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "HostName", default)]
    host_name: String,
    #[serde(rename = "DNSName", default)]
    dns_name: String,
    #[serde(rename = "OS", default)]
    os: String,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Vec<String>,
    #[serde(rename = "Online", default)]
    online: bool,
    #[serde(rename = "LastSeen", default)]
    last_seen: Option<String>,
    #[serde(rename = "RxBytes", default)]
    rx_bytes: u64,
    #[serde(rename = "TxBytes", default)]
    tx_bytes: u64,
}

impl RawPeer {
    fn into_peer(self, is_self: bool) -> Peer {
        Peer {
            id: self.id,
            hostname: self.host_name,
            dns_name: self.dns_name.trim_end_matches('.').to_string(),
            os: self.os,
            tailscale_ips: self.tailscale_ips,
            online: self.online,
            last_seen: self.last_seen,
            is_self,
            rx_bytes: self.rx_bytes,
            tx_bytes: self.tx_bytes,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct RawStatus {
    #[serde(rename = "Self")]
    self_peer: RawPeer,
    #[serde(rename = "Peer", default)]
    peer: HashMap<String, RawPeer>,
}

/// Full tailnet status: self + all known peers, online or not. Backs the
/// device/network statistics view.
pub fn status() -> Result<TailnetStatus> {
    let out = run(&["status", "--json"])?;
    let raw: RawStatus = serde_json::from_str(&out)?;
    let mut peers: Vec<Peer> = raw.peer.into_values().map(|p| p.into_peer(false)).collect();
    peers.sort_by(|a, b| a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()));
    Ok(TailnetStatus {
        self_peer: raw.self_peer.into_peer(true),
        peers,
    })
}

/// Valid `file cp` targets right now, as tailscaled itself sees them. Prefer
/// this over filtering `status()` for the send picker: it reflects ACLs and
/// file-sharing eligibility, not just online-ness.
pub fn cp_targets() -> Result<Vec<CpTarget>> {
    let out = run(&["file", "cp", "--targets"])?;
    Ok(parse_cp_targets(&out))
}

/// Parses `tailscale file cp --targets` output, e.g.:
/// `100.124.5.74\tpekarnya` (online) or `100.73.188.63\t11pm\toffline; last seen 981h1m0s ago`.
/// Split out from `cp_targets` so the format can be unit-tested without a
/// live `tailscaled` to shell out to.
fn parse_cp_targets(out: &str) -> Vec<CpTarget> {
    let mut targets = Vec::new();
    for line in out.lines() {
        let mut parts = line.splitn(3, '\t');
        let ip = parts.next().unwrap_or_default().trim().to_string();
        let name = parts.next().unwrap_or_default().trim().to_string();
        if ip.is_empty() || name.is_empty() {
            continue;
        }
        let rest = parts.next().unwrap_or_default().trim_start();
        let offline = rest.starts_with("offline");
        targets.push(CpTarget { ip, name, offline });
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_online_and_offline_targets() {
        let out = "100.73.188.63\t11pm\toffline; last seen 981h1m0s ago\n\
                    100.89.224.39\tcmf\toffline; last seen 1021h3m0s ago\n\
                    100.124.5.74\tpekarnya\n";
        let targets = parse_cp_targets(out);
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].ip, "100.73.188.63");
        assert_eq!(targets[0].name, "11pm");
        assert!(targets[0].offline);
        assert_eq!(targets[2].name, "pekarnya");
        assert!(!targets[2].offline);
    }

    #[test]
    fn ignores_blank_lines() {
        assert!(parse_cp_targets("\n\n").is_empty());
        assert!(parse_cp_targets("").is_empty());
    }
}

// --- sending ----------------------------------------------------------------

/// Send `file` to `target` (a hostname, DNS name, or IP from `cp_targets`).
/// `rename_to` overrides the filename the recipient sees (`--name`), used by
/// the GUI/CLI to attach `.tdmeta` sidecars alongside the real payload.
pub fn send_file(target: &str, file: &Path, rename_to: Option<&str>) -> Result<()> {
    if !file.is_file() {
        return Err(TaildropError::NotFound(file.display().to_string()));
    }
    let target_arg = format!("{}:", target);
    let file_str = file.to_string_lossy().into_owned();
    match rename_to {
        Some(name) => run(&["file", "cp", "--name", name, &file_str, &target_arg])?,
        None => run(&["file", "cp", &file_str, &target_arg])?,
    };
    Ok(())
}

// --- receiving ---------------------------------------------------------------

fn snapshot_dir(dir: &Path) -> HashSet<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// Drain whatever is currently waiting in the Tailscale inbox into `dest_dir`
/// (non-blocking: returns immediately if the inbox is empty) and report which
/// paths are new. Once this returns, the files are gone from tailscaled's
/// inbox for good, which is why callers should always point `dest_dir` at a
/// staging area they control, not the user's final save directory.
pub fn drain_inbox(dest_dir: &Path, conflict: ConflictPolicy) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dest_dir).map_err(|e| TaildropError::Io(e.to_string()))?;
    let dest_str = dest_dir.to_string_lossy().into_owned();
    let conflict_arg = format!("--conflict={}", conflict.as_flag());
    let before = snapshot_dir(dest_dir);
    run(&["file", "get", &conflict_arg, &dest_str])?;
    let after = snapshot_dir(dest_dir);
    Ok(after.difference(&before).cloned().collect())
}

/// Best-effort peek at what's waiting in the inbox *without* consuming it,
/// via the unstable `tailscale debug localapi` passthrough. Returns `None`
/// (never an error) if the running tailscaled doesn't support it, so callers
/// must treat this purely as an early "something's arriving" hint and keep
/// relying on `drain_inbox` as the source of truth.
pub fn peek_waiting_files() -> Option<Vec<WaitingFile>> {
    let out = run(&["debug", "localapi", "GET", "/localapi/v0/files"]).ok()?;
    serde_json::from_str(&out).ok()
}
