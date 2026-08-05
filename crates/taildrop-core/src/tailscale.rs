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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

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

/// Windows `CREATE_NO_WINDOW`. Not re-exported by std, so it's spelled out
/// here rather than pulling in `windows-sys` for a single constant.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Build a `Command` for the tailscale CLI.
///
/// `tailscale.exe` is a console binary, so on Windows every plain
/// `Command::new` allocates a console and flashes a black `cmd`-looking
/// window for the lifetime of the child. With the inbox poller shelling out
/// every few seconds that's a window popping over whatever the user is doing,
/// several times a minute — it looks like malware even though nothing is
/// wrong. `CREATE_NO_WINDOW` suppresses it; stdout/stderr are captured by the
/// callers regardless, so nothing is lost. No-op on other platforms.
#[cfg(windows)]
fn command(bin: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(bin);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
fn command(bin: &Path) -> Command {
    Command::new(bin)
}

fn run(args: &[&str]) -> Result<String> {
    let output = command(&binary())
        .args(args)
        .stdin(Stdio::null())
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

/// Like `run`, but kills the child and returns `TaildropError::Timeout`
/// instead of blocking forever. `tailscale file cp` to a peer that's
/// offline (or unreachable through NAT/DERP) has no bound on its own — it
/// just sits there, which from the GUI looks exactly like a hang. Output is
/// read only after the child exits (or is killed); `tailscale file cp`
/// never produces more than a line or two, so this can't deadlock on a full
/// pipe buffer the way it could for a chattier subprocess.
fn run_with_timeout(args: &[&str], timeout: Duration) -> Result<String> {
    run_bin_with_timeout(&binary(), args, timeout)
}

/// Split out from `run_with_timeout` so tests can point it at a fake
/// long-running binary instead of the real `tailscale` CLI (and without
/// mutating the process-wide `TAILSCALE_BIN` env var, which would race
/// across tests running in parallel).
fn run_bin_with_timeout(bin: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let mut child = command(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(TaildropError::Spawn)?;

    let status = match child
        .wait_timeout(timeout)
        .map_err(|e| TaildropError::Io(e.to_string()))?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(TaildropError::Timeout(format!(
                "`tailscale {}` timed out after {}s — the device may be offline",
                args.join(" "),
                timeout.as_secs()
            )));
        }
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }

    if !status.success() {
        let stderr = stderr.trim().to_string();
        return Err(TaildropError::Command(if stderr.is_empty() {
            format!("`tailscale {}` exited with {}", args.join(" "), status)
        } else {
            stderr
        }));
    }
    Ok(stdout)
}

// --- query cache ------------------------------------------------------------

/// How long a cached `status` / `file cp --targets` answer stays servable.
///
/// Every one of those calls costs a process spawn plus a round trip to
/// tailscaled — on Windows that is a few hundred milliseconds before the CLI
/// has even parsed its arguments (more with an antivirus inspecting each
/// spawn). The GUI asks for the same answer from several places: the tailnet
/// graph wants both, the device list and the send picker want one each, every
/// tab switch remounts a view, and a batch send used to re-ask for `status`
/// once per file. A device list a few seconds stale is invisible to the user;
/// the repeated spawns are not. Explicit refresh actions bypass this.
const CACHE_TTL: Duration = Duration::from_secs(10);

struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

static STATUS_CACHE: Mutex<Option<Cached<TailnetStatus>>> = Mutex::new(None);
static TARGETS_CACHE: Mutex<Option<Cached<Vec<CpTarget>>>> = Mutex::new(None);

/// Serve `slot` if it's younger than [`CACHE_TTL`], otherwise call `fetch` and
/// store the result. The lock is deliberately *not* held across `fetch`: a
/// wedged `tailscale` subprocess would otherwise block every other caller too,
/// and the worst case without it is one redundant spawn when two threads miss
/// at the same moment.
fn cached<T: Clone>(
    slot: &Mutex<Option<Cached<T>>>,
    fresh: bool,
    fetch: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if !fresh {
        let guard = slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = guard.as_ref() {
            if entry.fetched_at.elapsed() < CACHE_TTL {
                return Ok(entry.value.clone());
            }
        }
    }
    let value = fetch()?;
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Cached {
        value: value.clone(),
        fetched_at: Instant::now(),
    });
    Ok(value)
}

/// Populate the caches ahead of the first UI request. The GUI calls this on a
/// background thread at startup so the `tailscale` round trips overlap with
/// the webview booting instead of running after it.
pub fn warm_caches() {
    let _ = status(false);
    let _ = cp_targets(false);
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
        let dns_name = self.dns_name.trim_end_matches('.').to_string();
        let alias = dns_name
            .split('.')
            .next()
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.host_name.clone());
        Peer {
            id: self.id,
            hostname: self.host_name,
            alias,
            dns_name,
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
/// device/network statistics view. Answers from the [`CACHE_TTL`] cache unless
/// `fresh` is set, which user-initiated refreshes do.
pub fn status(fresh: bool) -> Result<TailnetStatus> {
    cached(&STATUS_CACHE, fresh, || {
        let out = run(&["status", "--json"])?;
        let raw: RawStatus = serde_json::from_str(&out)?;
        let mut peers: Vec<Peer> = raw.peer.into_values().map(|p| p.into_peer(false)).collect();
        peers.sort_by(|a, b| a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()));
        Ok(TailnetStatus {
            self_peer: raw.self_peer.into_peer(true),
            peers,
        })
    })
}

/// Valid `file cp` targets right now, as tailscaled itself sees them. Prefer
/// this over filtering `status()` for the send picker: it reflects ACLs and
/// file-sharing eligibility, not just online-ness. Cached like `status`.
pub fn cp_targets(fresh: bool) -> Result<Vec<CpTarget>> {
    cached(&TARGETS_CACHE, fresh, || {
        let out = run(&["file", "cp", "--targets"])?;
        Ok(parse_cp_targets(&out))
    })
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
    fn alias_prefers_first_dns_label_over_hostname() {
        let peer = RawPeer {
            id: "1".into(),
            host_name: "DESKTOP-ABC123".into(),
            dns_name: "renamed-machine.tailnet-name.ts.net.".into(),
            os: "linux".into(),
            tailscale_ips: vec![],
            online: true,
            last_seen: None,
            rx_bytes: 0,
            tx_bytes: 0,
        }
        .into_peer(false);
        assert_eq!(peer.alias, "renamed-machine");
        assert_eq!(peer.hostname, "DESKTOP-ABC123");
    }

    #[test]
    fn alias_falls_back_to_hostname_when_dns_name_missing() {
        let peer = RawPeer {
            id: "1".into(),
            host_name: "DESKTOP-ABC123".into(),
            dns_name: "".into(),
            os: "linux".into(),
            tailscale_ips: vec![],
            online: true,
            last_seen: None,
            rx_bytes: 0,
            tx_bytes: 0,
        }
        .into_peer(false);
        assert_eq!(peer.alias, "DESKTOP-ABC123");
    }

    #[test]
    fn ignores_blank_lines() {
        assert!(parse_cp_targets("\n\n").is_empty());
        assert!(parse_cp_targets("").is_empty());
    }

    /// Windows batch scripts aren't shell scripts, and this test only needs
    /// to prove the kill-on-timeout path works at all — Unix coverage is
    /// enough for that.
    #[test]
    #[cfg(unix)]
    fn run_with_timeout_kills_a_hung_process() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        let script_path =
            std::env::temp_dir().join(format!("taildrop-sleep-test-{}.sh", uuid::Uuid::new_v4()));
        {
            let mut f = std::fs::File::create(&script_path).unwrap();
            writeln!(f, "#!/bin/sh\nsleep 5\n").unwrap();
        }
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();

        let start = std::time::Instant::now();
        let result = run_bin_with_timeout(&script_path, &[], Duration::from_millis(150));
        let elapsed = start.elapsed();

        let _ = std::fs::remove_file(&script_path);

        assert!(matches!(result, Err(TaildropError::Timeout(_))));
        assert!(
            elapsed < Duration::from_secs(2),
            "should have been killed well before the script's 5s sleep, took {elapsed:?}"
        );
    }
}

// --- sending ----------------------------------------------------------------

/// How long to wait on a single `tailscale file cp` before giving up and
/// reporting the target unreachable. `tailscale file cp` itself has no
/// built-in bound when the peer is offline/unreachable — without this the
/// GUI just sits there indefinitely with no feedback.
const SEND_TIMEOUT: Duration = Duration::from_secs(25);

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
        Some(name) => run_with_timeout(&["file", "cp", "--name", name, &file_str, &target_arg], SEND_TIMEOUT)?,
        None => run_with_timeout(&["file", "cp", &file_str, &target_arg], SEND_TIMEOUT)?,
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
