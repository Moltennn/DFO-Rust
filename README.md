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

Status: **working spike.** It runs the current game build offline, serves the
player's own ARENA2 folder, keeps saves and settings in the same files the
official shell uses, and reaches the monitor's refresh rate (164 fps measured).
It has a small front door (play, game files, saves) but no updater, menus or
crash handling yet.

## How it works

The game is a web page. The official shell wraps it in Electron to do two
things a browser usually cannot: read ARENA2 from a folder, and store saves as
files. This shell does the same with much less machinery:

- **A GTK4 window** on WebKitGTK 6.0, from a private build with a small frame
  timing patch (the system library caps the page at 60 fps).
- **A small local web server** on a fixed port (47565), serving the game files
  and ARENA2.
- **A probe** injected into the page, used for measurements and for driving the
  UI without touching the mouse.
- **Files on disk** for saves and settings, in the official shell's own folders.

The port is fixed because the engine ties the game's stored data to the page
address, and that address includes the port. A random port would give the game
a fresh profile on every launch.

### Saves and settings

The page's storage is read from the server into memory when it loads, so reads
never wait on anything. Writes go straight to disk, so a failed write is
reported to the game at once. The server stores them as `Prefs/<key>` and
`Saves/SAVE<n>/` (`SaveData.txt`, `SaveInfo.txt`, `Screenshot.jpg`), written in
one step so a crash cannot leave a half file.

The folder is the official shell's own: `DAGGER_USER_DATA`, else
`$XDG_CONFIG_HOME/Daggerfall JavaScript`.

### The engine

WebKitGTK's frame timing comes from a 60 Hz fallback when it cannot see the
display's refresh, so the stock library runs the game at 60 fps. The shell
links a private build (2.54.2) with a small patch that uses the monitor's rate
instead; the system WebKit is untouched. Patch, build and caveats:
`notes/native-webkit-patch.md`.

The window is left to KWin, which opens it on the active output — the display
whose refresh rate the frame timing follows.

## Layout

```
src/lib.rs         shared: server, store, probe, state dir
src/main.rs        the GTK4 window and app
src/server.rs      the local server: game files, ARENA2, storage, control channel
src/store.rs       the file store (Prefs/ + Saves/)
src/mods.rs        optional DFO-Mods loader (spawn it, proxy its files)
src/webkit_shim.c  the WebKitGTK 6.0 calls the Rust side cannot bind
src/probe.js       injected page probe + the storage bridge
tools/extract-build.mjs   app.asar -> plain game/dist + launcher/
tools/build.sh     build against the private patched WebKitGTK
tools/run.sh       start the shell (one instance, fixed port)
tools/stop.sh      stop it
tools/enter.mjs    intro -> Offline -> Test Room -> a character, then measure
tools/ctl.mjs      evaluate one expression in the page, print the answer
tools/state.mjs    the one definition of the local state dir
game/              extracted build (git-ignored, not redistributed)
```

Local state lives outside the repository under `~/.local/share/dfo-rust`
(`DFO_STATE_DIR` overrides): the running URL, pid and log, and the store
backup. `AGENTS.md` has the details.

## Run it

Requirements: Rust, GTK4, and the private patched WebKitGTK 6.0 (built once,
see `notes/native-webkit-patch.md`; it installs to
`~/.local/webkitgtk-patched`). Node 18+ is needed only for the mods. ARENA2
comes from `~/.config/Daggerfall JavaScript/config.json` (`arena2Path`), or
`--arena2`.

```bash
# one-time: pull the current build out of the installed AppImage's app.asar
mkdir -p vendor
(cd vendor && /path/to/DFO-Online-linux-x86_64.AppImage \
  --appimage-extract 'resources/app.asar')
node tools/extract-build.mjs        # -> game/dist + game/launcher
rm -rf vendor

./tools/build.sh                    # cargo build against the private prefix
./tools/run.sh                      # launcher: Play, game files, saves
# or: ./tools/run.sh --no-launcher   # straight into the game (for the tools below)
node tools/enter.mjs                # walk to the Test Room and measure
./tools/stop.sh
```

`cargo build` without `tools/build.sh` warns: it will pick the system
WebKitGTK and run at 60 fps unless `DFO_ALLOW_SYSTEM_WEBKIT=1` says that is
wanted.

`node tools/ctl.mjs 'document.title'` evaluates one JavaScript expression in
the page and prints the answer.

The window takes `--width N --height N`, `--root DIR`, `--arena2 DIR`,
`--port N`, `--url URL` (load an existing page instead of the local origin),
`--no-launcher` (boot straight into the game) and `--serve-only` (the origin
without a window).

When `../DFO-Mods` is present, `tools/run.sh` starts the mod loader and serves
the game's files through it (`--no-mods` turns that off, `--mods DIR` picks
another workspace). The origin, the saves store and the control channel stay
local. The loader needs the build bundled in the AppImage: extract it in the
mods workspace once (`./DFO-Online-linux-x86_64.AppImage --appimage-extract`)
or point `DFO_APP` at an `app.asar`.

`/__dfo` evaluates JavaScript in the page. It binds to 127.0.0.1 only; it is a
development door, not a security boundary.

Mouse look is scaled to match the desktop game with a **linear** factor: on
the same save, window and hand sweep, Electron reported about 2.5× the raw
counts WebKitGTK receives under Wayland, so the game turned 2.5× slower here.
The default gain is 2.5; `DFO_MOUSE_GAIN` overrides it (`1` keeps the raw
deltas). This is a constant multiplier, not pointer acceleration.
**Alt+Enter** toggles fullscreen.

## Measured

The official shell and this one on the same machine, build and scene: Sentinel
(64 blocks / 568 draws), 1280×772 content, on the main display at 2560×1440 @
165 Hz, window focused. Frame rates are the mean of three 5-second runs.

| | official (Electron 42) | this shell (GTK4) |
|---|---|---|
| engine | Electron | WebKitGTK 2.54.2, private and patched |
| Sentinel | **165 fps** | **164 fps** |
| frame time | 6.06 ms | 6.1 ms |

The GTK4 result moves a little between runs (162–165 fps): its frame timing is
a timer rather than display-synced. The official shell is steady.

Test machine: Ryzen 7 5700X3D, Radeon RX 6750 XT, Mesa 26.2.4, Plasma 6 /
Wayland.

| measurement | result |
|---|---|
| first paint | ~0.7 s |
| world ready | ~26 s |
| 3D graphics (WebGL2) | yes |
| saves and settings | 96 real entries; Load Game lists the real characters |
| memory in world | game process ~2.5 GB, network ~90 MB |
| content size | 1280×772 |

These numbers assume hardware graphics. A machine that falls back to software
rendering will not hold 60 fps.

## Known gaps / next steps

1. **Display selection.** Frame timing follows the main display rather than the
   window's output: fine for play there, but a second patch would be needed for
   other outputs.
2. **Frame pacing.** The 165 Hz timing is a timer, not display-synced: the
   occasional tear is possible and runs vary 162–165 fps.

## Licensing

The game source and shell are MIT (Daggerfall Unity's notice included there).
ARENA2 belongs to the player and is never bundled; this repository ignores
`game/` and `vendor/` for that reason.
