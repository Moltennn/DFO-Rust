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
against `./tools/run.sh --serve-only`:

```
vivaldi-stable --user-data-dir="$HOME/.cache/dfo-rust/vivaldi" --no-first-run \
  --password-store=basic --remote-debugging-port=9222 \
  --app='http://127.0.0.1:47565/play/index.html?exterior&region=Sentinel&loc=Sentinel&shot&class=0' \
  --window-size=1280,772
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

## The heavy scene: Sentinel

The Test Room is a cave. The game's own router boots a full exterior city:

    /play/index.html?exterior&region=Sentinel&loc=Sentinel&shot&class=0

- `?exterior&region=<region>&loc=<loc>` assembles the location from MAPS.BSA
  (the default is Daggerfall city; `?region`/`?loc` alone are enough).
- `?shot` exposes `window.__playerEntity` and the other tool hooks.
- `?class=N` is the headless chargen skip; without it a chargen overlay covers
  the loaded city.

Sentinel, 64 blocks / 568 draws, 1280×772, focused:

| | WebKitGTK | Chromium (Vivaldi app-mode) |
|---|---|---|
| fps | 59 | **144** |
| median | 17 ms | 6.9 ms |
| p95 | 22 ms | 7.1 ms |
| worst | 83 ms | 7.6 ms |

The city's extra draws show as a slightly worse p95 (22 vs 18 ms in the Test
Room); the 60 Hz clock hides the rest. Chromium has no trouble with it.

Imperial City is not in MAPS.BSA (the travel map lists it under Cyrodiil as a
custom location): `?exterior&region=Cyrodiil&loc=Imperial City` fails with
"location not found", and the in-game travel route is 882 in-game hours and
killed the level-1 test character. Sentinel is a real MAPS.BSA region and the
heaviest ordinary city reachable this way.

## GTK4 / WebKitGTK 6.0 (2.54.2)

Installed after the full system update (321 packages). The probe is
`tools/gtk4-raf.py`; it builds the WebKit settings first, disables
`PreferPageRenderingUpdatesNear60FPS`, then loads the page.

Bare `raf.html`, 1280×772, focused, visible:

- GTK4 frame clock: `refresh_info` 6944 µs (144 Hz)
- near-60 feature: off at web-process construction
- rAF: 62.3 fps / 16.06 ms (t+4, t+12, t+22)
- Sentinel scene: 62.9 fps / 15.9 ms

GTK4's clock follows the output, but WebKitGTK's rAF does not. The cap is in
WebKitGTK's animation scheduling, not GTK's frame clock: the GTK4 port does not
lift it either. The native 144 fps route is closed.

## The actual source: the GLib display link

`WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=72` is rejected with "not a factor of
refresh rate 60fps" while the GTK clock is 6944 µs. The rate comes from
`DisplayLinkGLib`, which takes `DisplayVBlankMonitor::refreshRate()`. The DRM
vblank monitor is unavailable in this session, so it falls back to
`DisplayVBlankMonitorTimer`, whose constructor is hard-coded to
`FullSpeedFramesPerSecond` (60). That is the cap: not the rAF scheduler, not
GTK's clock, and not the output refresh.

The fallback is a small patch away (use `gdk_monitor_get_refresh_rate` for the
window's monitor, as the DRM path already does; WebKit bug 199376 comment 9
suggests exactly this). It still needs a WebKitGTK rebuild, and the GTK3 port
would also need a GTK4 shell because GTK3's own frame clock is 60.

## Output refresh, VSync and mixed monitors

The 60 Hz figure could have been VSync, or the mixed 144/165/60 Hz monitor
layout. Tested separately:

| configuration | GTK clock | rAF |
|---|---|---|
| DP-1, 144 Hz | 6944 µs (144 Hz) | 62.3 fps |
| DP-3, 165 Hz | 6067 µs (165 Hz) | 62.4 fps |
| Mesa `vblank_mode=0` | 6067 µs | 62.4 fps |
| XWayland (`GDK_BACKEND=x11`) | 16667 µs | 62.4 fps |
| `WEBKIT_DISABLE_COMPOSITING_MODE=1` | 6944 µs | 62.3 fps |
| gamescope nested `-r 144` | 16667 µs | 62.3 fps |

Gamescope's nested output reported 16667 µs, so that run is inconclusive about
a true 144 Hz nested compositor. The rest are clear: the GTK clock follows
144 Hz and 165 Hz, the window is focused and visible, and WebKit's rAF still
fires every ~16 ms.

The control that settles it: Chromium app-mode with
`--disable-gpu-vsync --disable-frame-rate-limit`, same Sentinel scene,
**243 fps / 4 ms median / 4.7 ms p95**. The page and the machine have the
headroom; the cap is WebKitGTK's animation scheduling, not VSync, not the
output refresh, and not the mixed-monitor layout. A compositor with a single
output is the one untested configuration; the frame-clock data makes it an
unlikely cause.

## What is left

- **Chromium app-mode** (Vivaldi/Chrome) with the local origin and an injected
  `daggerShell` script: 144 fps today, tiny, no bundled engine; the cost is
  depending on an installed browser and losing the native window/menu.
- **CEF**: 144 fps, but bundles Chromium again (~200 MB); no advantage over
  Electron except the Rust host.
- **Accept 60 fps** on Linux and keep wry/Tauri.
