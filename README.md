# DFO Online — a Rust shell experiment

> **Proof of concept — use at your own risk.** This is an experiment, not a
> supported client. It writes to the real `Daggerfall JavaScript/Prefs` and
> `Saves` folders. Back them up first. Not for online play.

An experimental, no-Electron desktop shell for **DFO Online**, the open-source
(Daggerfall-Unity-derived, MIT) web game. The game itself is unchanged: this
replaces only the `app/` shell, starting with offline play.

DFO Online is unofficial and not affiliated with Bethesda Softworks or ZeniMax
Media. See [NOTICE](NOTICE) for attribution, the upstream licences and the full
disclaimer. The shell's own code is MIT (see [LICENSE](LICENSE)).

Status: **spike, working.** It boots the current build (0.1.14473), runs it on
WebKitGTK, serves the player's own ARENA2 from disk, **backs the game's
localStorage-style store with the same `Saves/` + `Prefs/` files the Electron
shell uses**, and reports engine facts through a control channel. It does not
yet have the launcher, updater, menus or crash handling.

## Why this shape

The Electron shell is ~5,000 lines of Node around two things a browser
normally cannot do: read ARENA2 from a folder, and store saves as files. The
game page is already a web app, and the mod loader already serves it over
`http://127.0.0.1`. So the experiment is:

- **wry + tao** for the window (the same webview engine Tauri wraps; Tauri's
  framework is not needed for one window and a few commands),
- **axum** on a *pinned* local port for the game's origin,
- **WebKitGTK** as the engine on Linux (system dependency, not bundled),
- a small injected probe for measurements and text-based UI driving.

The port is pinned (47565) because WebKitGTK keys `localStorage` and IndexedDB
by origin, and an origin includes the port. A random port gives the game a
fresh profile every launch. That was measured, not assumed.

### The storage bridge

The page's seam speaks `localStorage`'s five words synchronously, and the
Electron preload answered them with synchronous file IO. This shell keeps the
contract:

- at document start, an initialization script seeds an in-memory map from
  `GET /__store/all` (a synchronous XHR - reads never block on IPC),
- `setItem` / `removeItem` post the write with a synchronous XHR, so a disk
  failure still throws at the caller (the game's quota contract),
- the server writes the same files (`src/store.rs`): `Prefs/<key>` with the
  shell's percent-encoded names, `Saves/SAVE<n>/SaveData.txt`, `SaveInfo.txt`,
  and `Screenshot.jpg` (data URLs translated), all temp-then-rename,
- `length()` rescans the index every 2 s, so a save copied into the folder
  appears, as it does under Electron.

The root is the Electron shell's own: `DAGGER_USER_DATA`, else
`$XDG_CONFIG_HOME/Daggerfall JavaScript`. Both shells therefore read and write
one store, which is what makes an A/B comparison with the same settings honest.

### Window placement

Wayland clients cannot position themselves. On KDE, the shell asks KWin to
`sendClientToScreen` the window (see `src/placement.rs`): `DFO_WINDOW` names an
output, `leftmost` (default), `<name>:fullscreen`, or `off`. That is the same
result the modding tools' `dfo place DP-1` gets with xdotool on XWayland.

## Layout

```
src/main.rs        the window, the probe, the IPC handler
src/server.rs      the local origin: dist/, arena2/, __store/, __dfo/
src/store.rs       the file store (Prefs/ + Saves/, same layout as Electron)
src/placement.rs   KWin: send the window to an output (Wayland)
src/probe.js       injected page probe + the daggerShell storage bridge
tools/extract-build.mjs   app.asar -> plain game/dist + launcher/
tools/run.sh       start (one instance, fixed port)
tools/stop.sh      stop
tools/enter.mjs    intro -> Offline -> Test Room -> a character, then measure
tools/ctl.mjs      evaluate one expression in the page, print the answer
tools/state.mjs    the one definition of the local state dir
game/              extracted build (git-ignored, not redistributed)
AGENTS.md          architecture, tooling, local state, the 60 Hz cap
```

Local state and comparison tooling live outside the repository, under
`~/.local/share/dfo-rust` (`DFO_STATE_DIR` overrides): the running URL, pid and
log, the store backup, the KWin scripts and the comparison screenshots.
`AGENTS.md` has the details.

## Run it

Requirements: Rust, `webkit2gtk-4.1`, `gtk3`. ARENA2 comes from the Electron
shell's `~/.config/Daggerfall JavaScript/config.json` (`arena2Path`), or
`--arena2 <dir>`.

```bash
# one-time: pull the current build out of the installed AppImage's app.asar
cd vendor
/path/to/DFO-Online-linux-x86_64.AppImage --appimage-extract 'resources/app.asar'
cd ..
node tools/extract-build.mjs

cargo build
./tools/run.sh
node tools/enter.mjs        # walk to the Test Room and measure
./tools/stop.sh
```

`node tools/ctl.mjs 'document.title'` is the inspection door: it queues a JS
expression that the page evaluates and answers. It exists because driving the
UI with `xdotool` moves the real cursor; this does not touch it.

## What has been measured

On this machine (AMD Ryzen 7 5700X3D, 8C/16T; Radeon RX 6750 XT; Mesa 26.2.4;
WebKitGTK 2.54.1; 1280×800):

| | |
|---|---|
| first paint / load | ~0.7 s (`load` event, local files) |
| `DOMContentLoaded` | ~0.3 s |
| world ready (Test Room) | ~26 s (world generation, same work as under Electron) |
| in-world frame rate | **60 fps**, median 17 ms, worst 43 ms |
| module workers | supported (`new Worker(..., {type:'module'})`) |
| `OffscreenCanvas` | supported |
| WebGL2 | yes, `Apple GPU` / `Apple Inc.` — WebKitGTK sanitizes the real adapter (system GL reports `radeonsi`/RX 6750 XT) |
| memory, in world | shell ~170 MB, WebKitWebProcess ~2.5 GB, network ~90 MB |
| file store | 96 keys from the player's real `Prefs/` + `Saves/`; the Load Game screen lists the real characters |
| window | on DP-1 (leftmost) at 320,471, 1280×800 content |

Those numbers are WebKitGTK *with a hardware context*. If a machine falls back
to llvmpipe, the same page will not hold 60 fps.

### Against Electron, same build and settings

Build 0.1.14473, same store (the player's real `Prefs/` + `Saves/`), 1280×772
content on the leftmost output (1920×1080 @ 144 Hz):

| | Electron 42 | Rust + WebKitGTK | Chromium (Vivaldi app-mode) |
|---|---|---|---|
| Test Room fps | 144 | **60** | 144 |
| Test Room median / p95 | 6.9 / 7.3 ms | 17 / 18 ms | 6.9 / 7.1 ms |
| Sentinel city fps | — | **59** | **144** |
| Sentinel median / p95 | — | 17 / 22 ms | 6.9 / 7.1 ms |
| total RSS (Test Room) | 2.9 GB | 3.64 GB | not measured |

The Sentinel scene is 64 blocks / 568 draws — the heaviest ordinary city the
scene router offers. The 60 fps cap is WebKitGTK's, not the page's frame cap:
the same page follows 144 Hz under Chromium on the same output. GTK4 +
WebKitGTK 6.0 does not help either: its frame clock is 144 Hz and the near-60
feature is off, yet rAF still measures ~62 fps (`tools/gtk4-raf.py`). There is
no memory win either; the game's heap dominates both. Details in
`notes/comparison.md` and `notes/experiment-3.md`.

## Known gaps / next steps

1. **Single instance.** The pinned port already refuses a second shell
   (`AddrInUse`); make it a real single-instance lock that raises the window.
2. **The launcher.** `game/launcher/` is extracted and unchanged; its bridge is
   two words (`onView`, `act`), easy to port once the game window is settled.
3. **ARENA2 detection and the folder picker** (Steam/GOG/DFU settings search) —
   the Electron shell's `arena2Detect.cjs` is the reference.
4. **The 144 fps question is settled for the native stack.** Sentinel was
   measured (64 blocks / 568 draws); GTK4 + WebKitGTK 6.0 was tested and also
   holds rAF at ~62 fps. The remaining 144 fps route is Chromium (app-mode or
   CEF), or accept 60 fps and keep the small native shell.
5. **Mod loader integration.** The mods patch the served bytes; a shell that
   serves `game/dist` can apply the same view/overlay before responding.
   The control channel is a placeholder for the `dfo` CDP tooling on Linux,
   which does not exist for WebKitGTK.

## Licensing

The game source and shell are MIT (Daggerfall Unity's notice included there).
ARENA2 belongs to the player and is never bundled; this repository ignores
`game/` and `vendor/` for that reason.
