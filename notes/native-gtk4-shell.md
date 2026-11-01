# The native GTK4 shell

Built and working. The shell runs the game on a GTK4 window and WebKitGTK 6.0
from the **private patched** WebKit (`notes/native-webkit-patch.md`), which is
what lifts the 60 Hz display-link cap. The earlier wry/GTK3 shell was dropped
once this one held the monitor's rate.

## Shape (as built)

- **Window**: `gtk4-rs` (`gtk::Application` + `ApplicationWindow`) in
  `src/main.rs`, with the launcher page in front of the game.
- **WebView**: WebKitGTK 6.0 through `src/webkit_shim.c`, because the
  `webkitgtk` Rust crate is an unpublished placeholder. The shim creates a
  `WebKitWebView` (6.0 dropped `webkit_web_view_new_with_settings`; settings are
  applied to a fresh view before the first load), disables
  `PreferPageRenderingUpdatesNear60FPS`, adds `src/probe.js` as a document-start
  user script and loads the URI.
- **Reuse**: `src/lib.rs` owns the origin, store, probe and state dir.
- **Storage**: unchanged — the page seeds from `/__store/all` and writes with
  synchronous XHR. Verified in the shell: `daggerShell.storage` round trip
  (set/get/remove) against the real store.
- **Single instance**: the pinned origin port refuses a second server, and
  `tools/run.sh` refuses to start while one is running (one store, one origin).
- **Placement**: left to KWin. The window opens on the active output; more
  importantly, WebKit's display link follows the display the page first sees,
  so moving the window does not repoint it (see the caveat below).

## Build and run

```bash
./tools/build.sh          # PKG_CONFIG_PATH=$PREFIX/lib/pkgconfig, webkitgtk-6.0
./tools/run.sh            # LD_LIBRARY_PATH=$PREFIX/lib
```

`DFO_WEBKIT_PREFIX` overrides `~/.local/webkitgtk-patched`. The C shim is
compiled by `cc` from the root `build.rs`; the WebKit web/network processes
come from `$PREFIX/lib/webkitgtk-6.0/` (compiled-in path).

## Measured (DP-3, 2560×1440 @ 165 Hz, 1280×772)

Same protocol as the README: Sentinel (64 blocks / 568 draws), three 5-second
runs, window focused.

| | |
|---|---|
| frame clock | 6067 µs (165 Hz) |
| Sentinel | **164 fps**, mean 6.1 ms (162–165 across runs) |
| canvas | 1280×772, dpr 1 |
| window | "DFO Online (Rust GTK4)", on DP-3 |

The display link creates one `DisplayVBlankMonitorTimer` per monitor at its
refresh (VG248 144, Odyssey G50A 165) — no 60 anywhere.

## Caveat: the link follows the page's first display

With a window on DP-1 (GTK clock 144) the page's display link still reports
165 (displayID 2, DP-3), even when DP-1 is primary, the window opens on DP-1,
or it is moved there afterwards. `ScreenManager` knows displayID 1 is the
VG248, but the page/link does not switch. Fine when playing on DP-3, which is
the main display here; a second display-link selection patch would be needed
for a 144 Hz DP-1 window. The debug build that showed this logged:

    DFO vblank displayID=1 model=VG248      refresh=144001
    DFO vblank displayID=2 model=Odyssey G50A refresh=164836

## Packaging and risks

- A private patched WebKit is ~150 MB; a Flatpak/AppImage can bundle it, or the
  patch goes upstream and the shell uses the system WebKit. A patched system
  library is not shippable; keep the private prefix for experiments.
- The fallback is a **timer**, not vblank-synced: right rate, possible
  tearing/jitter. The clean fix drives it from compositor frame callbacks
  (upstream work).
- No mature Rust WebKitGTK 6.0 bindings: the C shim is ours to maintain.
- Private WebKit builds need rebuilding for security updates; upstreaming the
  timer patch is what makes this maintainable.
