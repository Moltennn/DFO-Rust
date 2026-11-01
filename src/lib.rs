//! Shared shell pieces: the local origin, the file store, KWin placement and
//! the page probe. The wry/GTK3 binary and the native GTK4 shell both use them.

pub mod placement;
pub mod server;
pub mod store;

/// The page-side probe: engine facts, the daggerShell storage bridge and the
/// control channel. Injected at document start by each shell.
pub const PROBE: &str = include_str!("probe.js");

/// The shell's local state: the URL tools/ctl.mjs reads, and the pid/log
/// tools/run.sh keeps. Outside the repository and outside /tmp, so a restart
/// or a reboot does not lose it. Override with DFO_STATE_DIR.
pub fn state_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("DFO_STATE_DIR") {
        return dir.into();
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(home).join(".local/share"));
    base.join("dfo-rust")
}
