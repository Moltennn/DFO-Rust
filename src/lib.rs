//! Shared shell pieces: the local origin, the file store, KWin placement and
//! the page probe. The wry/GTK3 binary and the native GTK4 shell both use them.

pub mod placement;
pub mod server;
pub mod store;

/// The page-side probe: engine facts, the daggerShell storage bridge and the
/// control channel. Injected at document start by each shell.
pub const PROBE: &str = include_str!("probe.js");

/// Pointer-lock mouse gain. Under Wayland, WebKit's pointer lock receives raw
/// relative motion while the desktop game (Electron on X11) sees the
/// compositor's pointer acceleration, so the same hand motion turns much
/// slower. `DFO_MOUSE_GAIN` overrides the default; 1 keeps the raw deltas.
pub fn mouse_gain() -> f64 {
    std::env::var("DFO_MOUSE_GAIN")
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 8.0)
        .unwrap_or(2.0)
}

/// The initialization script handed to the webview: the shell configuration
/// followed by the probe, so the probe sees `__dfoMouseGain` first.
pub fn initialization_script() -> String {
    format!("globalThis.__dfoMouseGain = {};\n{PROBE}", mouse_gain())
}

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
