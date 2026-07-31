use clap::{Parser, Subcommand};
use std::path::PathBuf;
use taildrop_core::error::{Result, TaildropError};
use taildrop_core::store::Store;
use taildrop_core::{device_stats, poll_inbox_once, send_with_attribution, tailscale};

#[derive(Parser)]
#[command(name = "taildrop", version, about = "CLI companion for taildrop-gui")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send one or more files to a peer on your tailnet
    Send {
        /// Target hostname, DNS name, or IP (see `taildrop status` for options)
        target: String,
        /// File(s) to send
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// List, accept, or reject files waiting in your Taildrop inbox
    ConfirmDrop {
        #[command(subcommand)]
        action: ConfirmAction,
    },
    /// Show tailnet devices and this app's transfer stats per device
    Status,
}

#[derive(Subcommand)]
enum ConfirmAction {
    /// List files waiting for confirmation (polls the inbox first)
    List,
    /// Accept a pending file by id (or unambiguous id prefix)
    Accept { id: String },
    /// Reject a pending file by id (or unambiguous id prefix), deleting it
    Reject { id: String },
    /// Accept every currently pending file
    AcceptAll,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::new(None)?;
    match cli.command {
        Commands::Send { target, files } => cmd_send(&store, &target, &files),
        Commands::ConfirmDrop { action } => cmd_confirm_drop(&store, action),
        Commands::Status => cmd_status(&store),
    }
}

fn cmd_send(store: &Store, target: &str, files: &[PathBuf]) -> Result<()> {
    let status = tailscale::status()?;
    let mut any_failed = false;
    for file in files {
        match send_with_attribution(
            store,
            &status.self_peer.hostname,
            &status.self_peer.dns_name,
            target,
            file,
        ) {
            Ok(record) => {
                println!(
                    "sent {} to {} ({})",
                    record.file_name,
                    target,
                    taildrop_core::format::human_size(record.size)
                );
            }
            Err(e) => {
                any_failed = true;
                eprintln!("failed to send {}: {e}", file.display());
            }
        }
    }
    if any_failed {
        return Err(TaildropError::Command("one or more sends failed".into()));
    }
    Ok(())
}

fn cmd_confirm_drop(store: &Store, action: ConfirmAction) -> Result<()> {
    match action {
        ConfirmAction::List => {
            let settings = store.load_settings()?;
            poll_inbox_once(store, settings.conflict_policy)?;
            let pending = store.list_pending()?;
            if pending.is_empty() {
                println!("no files waiting for confirmation");
                return Ok(());
            }
            for item in pending {
                let sender = item.sender_hostname.as_deref().unwrap_or("unknown sender");
                println!(
                    "{}  {} <- {}  {}",
                    &item.id[..8],
                    item.file_name,
                    sender,
                    taildrop_core::format::human_size(item.size)
                );
            }
        }
        ConfirmAction::Accept { id } => {
            let settings = store.load_settings()?;
            let full_id = resolve_pending_id(store, &id)?;
            let record = store.accept_pending(&full_id, &settings)?;
            println!(
                "accepted {} -> {}",
                record.file_name,
                record.saved_path.unwrap_or_default()
            );
        }
        ConfirmAction::Reject { id } => {
            let full_id = resolve_pending_id(store, &id)?;
            let record = store.reject_pending(&full_id)?;
            println!("rejected {}", record.file_name);
        }
        ConfirmAction::AcceptAll => {
            let settings = store.load_settings()?;
            let pending = store.list_pending()?;
            if pending.is_empty() {
                println!("no files waiting for confirmation");
                return Ok(());
            }
            for item in pending {
                let record = store.accept_pending(&item.id, &settings)?;
                println!(
                    "accepted {} -> {}",
                    record.file_name,
                    record.saved_path.unwrap_or_default()
                );
            }
        }
    }
    Ok(())
}

fn resolve_pending_id(store: &Store, prefix: &str) -> Result<String> {
    let pending = store.list_pending()?;
    let matches: Vec<_> = pending.iter().filter(|i| i.id.starts_with(prefix)).collect();
    match matches.as_slice() {
        [] => Err(TaildropError::NotFound(format!(
            "no pending file matches id \"{prefix}\" (run `taildrop confirm-drop list`)"
        ))),
        [single] => Ok(single.id.clone()),
        _ => Err(TaildropError::Command(format!(
            "\"{prefix}\" matches more than one pending file, use a longer id"
        ))),
    }
}

fn cmd_status(store: &Store) -> Result<()> {
    let stats = device_stats(store)?;
    println!(
        "{:<20} {:<9} {:<8} {:<10} {:>12} {:>12}",
        "DEVICE", "OS", "STATUS", "IP", "SENT", "RECEIVED"
    );
    for d in stats.devices {
        let label = if d.peer.is_self {
            format!("{} (this device)", d.peer.hostname)
        } else {
            d.peer.hostname.clone()
        };
        let online = if d.peer.online { "online" } else { "offline" };
        let ip = d.peer.tailscale_ips.first().cloned().unwrap_or_default();
        println!(
            "{:<20} {:<9} {:<8} {:<10} {:>5} ({:>5}) {:>5} ({:>5})",
            label,
            d.peer.os,
            online,
            ip,
            d.sent_count,
            taildrop_core::format::human_size(d.sent_bytes),
            d.received_count,
            taildrop_core::format::human_size(d.received_bytes),
        );
    }
    Ok(())
}
