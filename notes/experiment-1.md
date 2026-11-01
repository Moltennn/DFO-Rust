# Experiment 1 — WebKitGTK viability

Date: 2026-11-01. Build: DFO Online **0.1.14461** (the current AppImage's
`app.asar`, extracted to plain files; 32,579 files / 347.2 MiB).

Machine: 8-core Ryzen, AMD RX 6750 XT (`radeonsi`, Mesa 26.2.4, kernel 7.2.9),
Plasma Wayland, WebKitGTK **2.54.1**, GTK 3.24.52, Rust 1.99.

## What ran

- `wry 0.57` + `tao 0.37`, one 1280×800 window.
- `axum 0.8` serving `game/dist` and `/arena2/<NAME>` from
  `~/.config/Daggerfall JavaScript/config.json`'s `arena2Path`.
- The unmodified game page, browser mode (`daggerShell` absent → `localStorage`).

## Results

- Boot to the intro: `DOMContentLoaded` 291–298 ms, `load` 715–727 ms.
- Intro → main menu → Offline → Test Room → Nord Warrior: all clickable by
  text through the injected control channel.
- World ready after 26.3 s (world generation).
- In world: **60 fps**, median frame 17 ms, worst 43 ms, 1280×800, no pointer
  lock (programmatic clicks do not grant one).
- `new Worker(url, {type:'module'})`, `OffscreenCanvas`, WebGL2 all present.
- Memory in world: shell 169 MB, `WebKitWebProcess` 2.49 GB, network 88 MB.

## Gotchas found (all fixed or documented)

1. **Wayland needs `build_gtk`.** `WebViewBuilder::build(&window)` returns
   `UnsupportedWindowHandle` on Linux; `build_gtk(window.default_vbox())` is the
   one that works. `gtk_window()` as the container trips
   `Gtk-WARNING: ... can only contain one widget at a time` (tao already put a
   `GtkBox` there) — the webview must go in that box.
2. **The origin is the storage identity.** WebKitGTK wrote
   `localstorage/http_127.0.0.1_39473.localstorage` on a random port. A random
   port = a new profile every launch (settings, IndexedDB caches, ARENA2
   ingest). The port is now pinned to 47565.
3. **The renderer string is sanitized.** `WEBGL_debug_renderer_info` returns
   `Apple GPU` / `Apple Inc.` in WebKitGTK (anti-fingerprinting), so it cannot
   be used to tell hardware from software. `glxinfo -B` (or `eglinfo`) answers
   that on the system side; 60 fps answers it in practice.
4. **The control channel must not touch the game's canvas.**
   `getContext('webgl2', ...)` on the game's canvas would create it with the
   probe's attributes. The probe uses a throwaway 1×1 canvas for adapter info and
   only reads dimensions from the game's.

## Open questions

- Same build under Electron on this machine, for a like-for-like FPS/RAM
  comparison (the game's own `--remote-debugging-port=9222` route).
- FPS in a city/dungeon with many sprites and lighting, not just the Test Room.
- Whether WebKitGTK's `localStorage` quota (commonly 5 MB per origin) is enough
  for a save slot; the Electron saves on this machine are ~1 MB per slot.
  This decides how urgent the file-backed bridge is.
- Whether the game's WebGL2 path hits any WebKitGTK-specific rendering bugs in
  longer play (the intro and Test Room rendered correctly).
