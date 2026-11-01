# Experiment 3 — the 60 Hz cap

Machine: AMD Ryzen 7 5700X3D (8C/16T), Radeon RX 6750 XT (`radeonsi`, Mesa
26.2.4, kernel 7.2.9), Plasma 6 / KWin 6.7.5 on Wayland, WebKitGTK **2.54.1**,
GTK 3.24.52, Rust 1.99. Output DP-1: 1920×1080 @ 144 Hz.

The question: can WebKitGTK's `requestAnimationFrame` be lifted off 60 Hz on
this machine, so a Linux shell can match Electron's 144 fps?

## What was tried

All runs were focused, on DP-1, 1280×772 content, in the Test Room (or the
bare `raf.html` noted below); 3 s rAF sample after warm-up.

| configuration | fps | median |
|---|---|---|
| baseline | 59–60 | 17 ms |
| `--no-near-60-fps` (feature toggle) | 59 | 17 ms |
| `GDK_GL=disable` | 54–57 | 18 ms |
| `--no-near-60-fps` + `GDK_GL=disable` | 57 | 18 ms |
| `GDK_BACKEND=x11` (XWayland) | 62 | 16 ms |
| both + `WEBKIT_DISABLE_COMPOSITING_MODE=1` | 63 | 16 ms |
| bare `raf.html`, both | 60–61 | 16 ms |

The GTK frame clock itself, printed at startup via
`gdk_frame_clock_get_refresh_info`: **16667 µs (59 Hz) in every configuration**,
including `GDK_GL=disable` and XWayland. The clock never follows the 144 Hz
output, so no page-side or WebKit-side setting can get a frame more often.

## The feature toggle (kept as a knob)

The WebKit setting is `PreferPageRenderingUpdatesNear60FPS`
(`webkit_feature_get_identifier`, default true). The Rust `webkit2gtk` 2.0
crate has no binding for the WebKitFeature API, so `src/main.rs` calls
`webkit_settings_get_all_features` / `webkit_feature_list_find` /
`webkit_settings_set_feature_enabled` directly. It is applied before the game's
document loads (the webview is built on `about:blank` first) and reports
`off: true`. It does not lift the cap on its own; it is required by the GTK4
test and is the documented knob.

## The Chromium ceiling

The same page and the same origin, opened in Vivaldi (Chromium) app-mode
against `./target/debug/dfo-shell --serve-only`:

```
vivaldi-stable --user-data-dir=/tmp/opencode/vivaldi-dfo --no-first-run \
  --password-store=basic --remote-debugging-port=9222 \
  --app=http://127.0.0.1:47565/play/index.html --window-size=1280,800
```

Test Room, 1280×800: **144 fps**, median 6.9 ms, p95 7.1 ms, worst 15 ms
(window unfocused). The page is not the limiter; GTK3 is.

## Why the workaround in the WebKit bug did not work here

[WebKit bug 199376](https://bugs.webkit.org/show_bug.cgi?id=199376), comment 8
(2026-10-12) measured 8–9 ms with `GDK_GL=disable` **and** the feature off on
GNOME 50.5/Mutter, 120 Hz, a bare web view. Under KWin 6.7.5 the frame clock
stays at 16667 µs even with the same combination. The likely reason is that
WebKit's web view still draws through its own GL path (WebGL/compositing), so
GTK 3's window keeps the GL frame-clock branch; the GTK bug they cite
(gtk#8461) is about GL windows never requesting Wayland frame callbacks.

## What is left

- **GTK4 + WebKitGTK 6.0**: the proper fix (GTK4's frame clock requests
  Wayland callbacks). Not installed; `webkitgtk-6.0` 2.54.2 is in `extra`.
  This system has **321 pending updates**, so a full `pacman -Syu` is needed
  before installing it (never a partial upgrade). wry/tao are GTK3-only, so
  this means a GTK4 shell (gtk4-rs + a WebKitGTK 6.0 FFI shim, since the
  `webkitgtk` crate is an unpublished placeholder), or a C/GTK4 shim.
- **Chromium app-mode** (Vivaldi/Chrome) with the local origin and an injected
  `daggerShell` script: 144 fps today, tiny, no bundled engine; the cost is
  depending on an installed browser and losing the native window/menu.
- **CEF**: 144 fps, but bundles Chromium again (~200 MB); no advantage over
  Electron except the Rust host.
- **Accept 60 fps** on Linux and keep wry/Tauri.
