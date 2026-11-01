# Native GTK4 shell (after the patched WebKit spike)

Only worth starting if `$STATE/webkit-patched/run-probe.sh` reaches ~144 fps on
DP-1. The current shell is `wry`/`tao` (GTK3), and GTK3's own frame clock is
60 Hz; the patched WebKit only helps a GTK4 window.

## Shape

- **Window**: `gtk4-rs` (`gtk::Application` + `ApplicationWindow`).
- **WebView**: WebKitGTK 6.0 through a small **C shim** (the `webkitgtk` Rust
  crate is an unpublished placeholder). The shim exposes only what the shell
  uses:
  - create a `WebKitWebView` with a `WebKitSettings` (the near-60 feature can
    be disabled before the web process starts);
  - load a URI;
  - add a user script at document start (the probe);
  - read page load state / close;
  - optionally `evaluate_javascript` for diagnostics.
- **Reuse**: `src/server.rs`, `src/store.rs`, `src/probe.js`, `src/placement.rs`
  and the tools/state dir. Only the window/webview layer changes.
- **Storage**: unchanged — the served page loads `/__store/all` and writes via
  synchronous XHR; no preload/bridge.
- **Placement**: KWin `sendClientToScreen` as `src/placement.rs` does.
- **Single instance**: the pinned origin port already refuses a second server;
  add a lock file under `$STATE` and raise the window later.

## Build and link (private WebKit)

- Configure against the private prefix:
  `PKG_CONFIG_PATH=$PREFIX/lib/pkgconfig`
  `webkitgtk-6.0` (or the `webkitgtk6` pkg-config name) + `gtk4`.
- Runtime:
  `LD_LIBRARY_PATH=$PREFIX/lib`,
  `GI_TYPELIB_PATH` only for Python tests,
  and the web process under `$PREFIX/lib/webkit2gtk-6.0/` (compiled in).
- The C shim is compiled by `cc` in `build.rs` or a small `Makefile`; link it
  into the Rust binary.

## Packaging

- A private patched WebKit is ~150 MB; a Flatpak/AppImage can bundle it, or the
  patch goes upstream and the shell uses the system WebKit.
- A patched system library is not shippable; keep the private prefix/local
  package for experiments only.

## Tests

- `tools/gtk4-raf.py` against the private prefix: engine rate.
- `tools/enter.mjs` through the control channel: the real shell's Test Room.
- `cargo test` for the store; the existing `check.mjs`/anchor workflow for mods
  once the shell serves the same build.

## Risks

- The patched display link is a **timer**, not vblank-synced: 144 fps with
  possible tearing/jitter. The clean fix drives it from compositor frame
  callbacks (upstream work).
- No mature Rust WebKitGTK 6.0 bindings: the C shim is ours to maintain.
- Private WebKit builds need rebuilding for security updates; upstreaming the
  timer patch is what makes this maintainable.
