//! Smallest viable subset of REQ:codon/fish-shell-integration: a
//! per-process Unix-domain socket that the fish plugin's `#@`
//! handler dials over to ask the running codon agent for a command
//! suggestion.
//!
//! Scope intentionally narrow — covers only the `agent.complete`
//! method, no per-window sockets, no `codon do` dispatch, no
//! context injection / redaction / trace. Those follow in the
//! sibling phase-22 tasks.
//!
//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary.

mod agent_complete;
mod server;

use gpui::App;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Embedded fish-plugin source. Surfaced via `plugin_source()` so a
/// later `codon fish-init` CLI subcommand can write it into
/// `~/.config/fish/conf.d/codon.fish` without locating the file on
/// disk.
pub const PLUGIN_SOURCE: &str = include_str!("../share/codon.fish");

static SOCK_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Returns the socket path codon advertised in `CODON_SOCK` for this
/// process, if the listener bound successfully.
pub fn socket_path() -> Option<&'static Path> {
    SOCK_PATH.get().map(PathBuf::as_path)
}

/// Spawn the per-process Unix-socket RPC server and export
/// `CODON_SOCK` so child PTYs inherit it through
/// `insert_zed_terminal_env`.
///
/// Failures are logged and swallowed: the rest of codon stays usable
/// even if the socket can't bind (e.g. read-only `$XDG_RUNTIME_DIR`
/// on an exotic platform).
pub fn init(cx: &mut App) {
    let path = match choose_socket_path() {
        Some(path) => path,
        None => {
            log::warn!("codon-fish: could not pick a socket path; #@ integration disabled");
            return;
        }
    };
    if path.exists() {
        // A previous codon process crashed without unlinking; we own
        // the path by convention (it's PID-scoped) so reclaim it.
        if let Err(err) = std::fs::remove_file(&path) {
            log::warn!(
                "codon-fish: could not remove stale socket {:?}: {err}",
                path
            );
        }
    }
    let listener = match net::async_net::UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) => {
            log::warn!("codon-fish: failed to bind {:?}: {err}", path);
            return;
        }
    };
    // SAFETY: `init` runs once during codon's single-threaded
    // startup, before any worker thread or PTY spawn. This is the
    // canonical place to propagate the address into child
    // environments.
    unsafe {
        std::env::set_var("CODON_SOCK", &path);
    }
    if SOCK_PATH.set(path.clone()).is_err() {
        log::warn!("codon-fish: init called twice; ignoring");
        return;
    }
    log::info!("codon-fish: listening on {:?}", path);
    cx.spawn(async move |cx| {
        server::run(listener, cx.clone()).await;
    })
    .detach();
}

fn choose_socket_path() -> Option<PathBuf> {
    let pid = std::process::id();
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| std::env::var_os("TMPDIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    Some(dir.join(format!("codon-{pid}.sock")))
}
