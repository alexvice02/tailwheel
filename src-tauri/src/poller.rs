use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use taildrop_core::format::human_size;
use taildrop_core::models::{ConflictPolicy, PendingIncoming, Settings};
use taildrop_core::store::Store;
use taildrop_core::tailscale;

/// A `tailscale file get --loop` child that drains the inbox into staging for
/// as long as the app runs, plus the flag telling the rest of the app whether
/// it is actually up.
///
/// Why a child at all: the previous design ran `tailscale file get` on every
/// tick, which at the default 3s interval is ~1200 process spawns an hour for
/// an inbox that is almost always empty. That is cheap on Linux (~15ms a call)
/// and expensive on Windows, where each spawn pays for process creation,
/// antivirus inspection and a fresh connection to tailscaled — and flashes a
/// console window while it does. One long-lived child costs one of those, ever.
#[derive(Default)]
pub struct Receiver {
    child: Mutex<Option<Child>>,
    /// False while falling back to per-tick draining (an older `tailscale`
    /// without `--loop`, or a daemon that isn't up yet).
    pub alive: AtomicBool,
}

impl Receiver {
    /// Kill the child. Must be called before the process exits: on Windows a
    /// child is not reaped with its parent, so an orphaned receiver would go
    /// on quietly draining the user's inbox into a staging directory that no
    /// running app is watching.
    pub fn shutdown(&self) {
        if let Some(mut child) = self
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
            tailscale::log_event("receiver: stopped");
        }
    }

    /// Make sure the child is up, starting it if it never ran or has died.
    fn ensure_running(&self, store: &Store, conflict: ConflictPolicy) -> ReceiverState {
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => return ReceiverState::AlreadyRunning,
                Ok(Some(status)) => {
                    tailscale::log_event(&format!("receiver: exited with {status}"));
                    *guard = None;
                }
                Err(e) => {
                    tailscale::log_event(&format!("receiver: lost track of child: {e}"));
                    *guard = None;
                }
            }
        }

        match tailscale::spawn_receiver(&store.staging_dir(), conflict) {
            Ok(child) => {
                *guard = Some(child);
                ReceiverState::Started
            }
            Err(e) => {
                tailscale::log_event(&format!("receiver: spawn failed: {e}"));
                ReceiverState::Failed
            }
        }
    }
}

enum ReceiverState {
    AlreadyRunning,
    Started,
    Failed,
}

/// Background loop that surfaces new arrivals to the frontend (event) and the
/// OS (notification). Runs on a plain OS thread rather than async: everything
/// it calls blocks on file IO anyway, so there's nothing to gain from tokio.
///
/// It no longer does the draining itself — the `Receiver` child does, and this
/// loop only reads the staging directory to see what landed. When the child
/// can't be kept alive it falls back to the old per-tick
/// `poll_inbox_once` drain so an older `tailscale` still works, just noisier.
pub fn spawn(app_handle: AppHandle, store: Arc<Store>, receiver: Arc<Receiver>) {
    std::thread::spawn(move || {
        // Three quick deaths in a row means `--loop` isn't going to work on
        // this machine (it predates the flag, most likely). Stop retrying and
        // drain per tick instead of respawning a doomed child forever.
        const MAX_QUICK_DEATHS: u32 = 3;
        const QUICK_DEATH: Duration = Duration::from_secs(3);

        let mut use_receiver = true;
        let mut quick_deaths = 0u32;
        let mut spawned_at: Option<Instant> = None;
        let mut running_conflict: Option<ConflictPolicy> = None;

        loop {
            let settings = match store.load_settings() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("taildrop: failed to load settings: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };

            if let Err(e) = store.prune_history(settings.history_retention, chrono::Utc::now()) {
                eprintln!("taildrop: history prune failed: {e}");
            }

            // The child was started with a `--conflict` flag; a settings change
            // only takes effect if we restart it with the new one.
            if running_conflict.is_some_and(|c| c != settings.conflict_policy) {
                receiver.shutdown();
                running_conflict = None;
            }

            if use_receiver {
                match receiver.ensure_running(&store, settings.conflict_policy) {
                    ReceiverState::AlreadyRunning => {
                        receiver.alive.store(true, Ordering::Relaxed);
                    }
                    ReceiverState::Started => {
                        // If the child we just replaced barely outlived its
                        // own startup, that's how an unsupported `--loop`
                        // shows up: it exits before it can do any work.
                        if spawned_at.is_some_and(|at| at.elapsed() < QUICK_DEATH) {
                            quick_deaths += 1;
                        } else {
                            quick_deaths = 0;
                        }
                        spawned_at = Some(Instant::now());
                        receiver.alive.store(true, Ordering::Relaxed);
                        running_conflict = Some(settings.conflict_policy);
                    }
                    ReceiverState::Failed => {
                        quick_deaths += 1;
                        receiver.alive.store(false, Ordering::Relaxed);
                        running_conflict = None;
                    }
                }
                if quick_deaths >= MAX_QUICK_DEATHS {
                    use_receiver = false;
                    receiver.alive.store(false, Ordering::Relaxed);
                    tailscale::log_event(
                        "receiver: giving up on `file get --loop`, draining per tick instead",
                    );
                }
            }

            let polled = if receiver.alive.load(Ordering::Relaxed) {
                taildrop_core::poll_staging_once(&store)
            } else {
                taildrop_core::poll_inbox_once(&store, settings.conflict_policy)
            };

            match polled {
                Ok(new_items) if !new_items.is_empty() => {
                    announce(&app_handle, &store, &settings, &new_items)
                }
                Ok(_) => {}
                Err(e) => eprintln!("taildrop: inbox poll failed: {e}"),
            }

            std::thread::sleep(Duration::from_secs(settings.poll_interval_secs.max(1)));
        }
    });
}

fn announce(
    app_handle: &AppHandle,
    store: &Store,
    settings: &Settings,
    new_items: &[PendingIncoming],
) {
    if settings.auto_accept {
        for item in new_items {
            match store.accept_pending(&item.id, settings) {
                Ok(record) => {
                    let body = format!("Received {} ({})", record.file_name, human_size(record.size));
                    let _ = app_handle
                        .notification()
                        .builder()
                        .title("Taildrop")
                        .body(body)
                        .show();
                }
                Err(e) => eprintln!("taildrop: auto-accept failed: {e}"),
            }
        }
        let _ = app_handle.emit("history-updated", ());
    } else {
        for item in new_items {
            let body = match &item.sender_hostname {
                Some(sender) => format!("{sender} sent you {}", item.file_name),
                None => format!("New file received: {}", item.file_name),
            };
            let _ = app_handle
                .notification()
                .builder()
                .title("Taildrop")
                .body(body)
                .show();
        }
        let _ = app_handle.emit("pending-updated", new_items);
    }
}
