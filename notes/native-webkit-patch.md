# Native spike: patched WebKitGTK 6.0

Goal: a **native GTK4 shell at 144 fps**, without Chromium. The 60 Hz cap is
not GTK4's frame clock; it is WebKitGTK's GLib display link:

- `DisplayLinkGLib` asks `DisplayVBlankMonitor` for the rate.
- The DRM vblank monitor is unavailable here (`drmWaitVBlank` fails for a
  Wayland client that is not the DRM master).
- `DisplayVBlankMonitorTimer` then uses `FullSpeedFramesPerSecond` — 60.

`WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=144` is rejected with "refresh rate
60fps" while the GTK clock is 6944 µs; a single-output run is the same. That is
the whole cap.

## The patch

`notes/native/0001-display-link-timer-refresh.patch`, against WebKitGTK
2.54.2:

- `DisplayVBlankMonitorTimer` takes the refresh rate (`create(unsigned)`,
  constructor parameter) instead of hard-coding 60.
- `DisplayVBlankMonitor::create()` uses the monitor's refresh when the DRM
  vblank monitor fails: `ScreenManager::screen(displayID)` +
  `gdk_monitor_get_refresh_rate()` (mHz) / 1000.

Apply in the source root:

    patch -p1 < notes/native/0001-display-link-timer-refresh.patch

This is a **timer** fallback, not vblank-synced: 144 fps with possible
tearing/jitter. A proper upstream fix would drive the display link from the
compositor's frame callbacks.

## Build (private, no system replacement)

Local tree: `$STATE/webkit-patched/` (source 2.54.2, patch, `build.sh`,
`run-probe.sh`). It installs to `~/.local/webkitgtk-patched` by default:
`DFO_WEBKIT_PREFIX` overrides.

Build packages (Arch):

    sudo pacman -S --needed cmake gobject-introspection glib2-devel gperf \
      ruby ruby-stdlib unifdef wayland-protocols

`clang`, `lld`, `ninja`, `gst-plugins-bad` are already installed. Then:

    ~/.local/share/dfo-rust/webkit-patched/build.sh

The script configures Release with `PORT=GTK`, `USE_GTK4=ON`, docs/minibrowser
off, and `CMAKE_INSTALL_PREFIX=$PREFIX`. Expect 1–2 hours and ~10 GB.

## Test

    ~/.local/share/dfo-rust/webkit-patched/run-probe.sh

It sets `LD_LIBRARY_PATH` and `GI_TYPELIB_PATH` to the private prefix and runs
`tools/gtk4-raf.py`. A quick check of the link's rate:

    WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=144 \
      ~/.local/share/dfo-rust/webkit-patched/run-probe.sh

With the patch, the throttle value is accepted (the refresh is 144) and rAF
should be ~144 on DP-1; the GTK4 clock is already 144.

## If it works

The shell must move to **GTK4** (wry/tao are GTK3-only):

- `gtk4-rs` window + a small C shim (or raw FFI) for WebKitGTK 6.0
  (`WebKitWebView`, load URI, settings/feature, evaluate JS, signals); the
  `webkitgtk` Rust crate is an unpublished placeholder.
- The existing `src/server.rs`, `src/store.rs` and `src/probe.js` carry over;
  only the window/webview layer is replaced.
- Packaging: a private patched WebKit (~150 MB) via Flatpak/AppImage, or the
  upstream patch. A patched system library is not shippable.

If the probe does not reach 144, stop before the shell work.
