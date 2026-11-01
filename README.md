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
official shell uses, and in its native GTK4 form reaches the monitor's refresh
rate (165 fps measured). It has no launcher, updater, menus or crash handling
yet.

## How it works

The game is a web page. The official shell wraps it in Electron to do two
things a browser usually cannot: read ARENA2 from a folder, and store saves as
files. This shell does the same with much less machinery:

- **A window** on WebKitGTK, the system web engine.
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
`$XDG_CONFIG_HOME/Daggerfall JavaScript`. Both shells read and write the same
store.

### Window placement

Wayland clients cannot place themselves. On KDE, the wry shell asks KWin to
send the window to a chosen output (`src/placement.rs`): `DFO_WINDOW` names an
output, `leftmost` (default), `<name>:fullscreen`, or `off`.

The native GTK4 shell leaves placement to KWin, so the window opens on the
active output — the display whose refresh rate its frame timing follows.

## The two shells

| | `wry` (default) | `gtk4-shell` (native) |
|---|---|---|
| window | GTK3 | GTK4 |
| engine | system WebKitGTK 4.1 | privately built WebKitGTK 6.0, patched |
| frame rate | 60 fps (engine limit) | the monitor's rate (165 fps measured) |
| build | `cargo build` | `./tools/build-native.sh` |
| run | `./tools/run.sh` | `./tools/run-native.sh` |

The system WebKit is never modified. The GTK4 shell links a private copy of
WebKitGTK whose frame timing uses the monitor's refresh rate instead of a
hard-coded 60. Patch and build steps: `notes/native-webkit-patch.md`.

## Layout

```
src/lib.rs         shared: server, store, placement, probe, state dir
src/main.rs        the wry/GTK3 window (default feature)
src/server.rs      the local server: game files, ARENA2, storage, control channel
src/store.rs       the file store (Prefs/ + Saves/)
src/placement.rs   KWin: send the window to an output (Wayland)
src/probe.js       injected page probe + the storage bridge
gtk4-shell/        native GTK4 + WebKitGTK 6.0 shell and its C shim
tools/extract-build.mjs   app.asar -> plain game/dist + launcher/
tools/run.sh       start the wry shell (one instance, fixed port)
tools/run-native.sh       start the GTK4 shell
tools/stop.sh      stop either shell
tools/enter.mjs    intro -> Offline -> Test Room -> a character, then measure
tools/ctl.mjs      evaluate one expression in the page, print the answer
tools/state.mjs    the one definition of the local state dir
game/              extracted build (git-ignored, not redistributed)
```

Local state lives outside the repository under `~/.local/share/dfo-rust`
(`DFO_STATE_DIR` overrides): the running URL, pid and log, the store backup and
the placement scripts. `AGENTS.md` has the details.

## Run it

Requirements: Rust, `webkit2gtk-4.1`, `gtk3`. ARENA2 comes from
`~/.config/Daggerfall JavaScript/config.json` (`arena2Path`), or `--arena2`.

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

`node tools/ctl.mjs 'document.title'` evaluates one JavaScript expression in
the page and prints the answer.

### Native GTK4 shell

Needs the patched WebKitGTK 6.0 built first (see
`notes/native-webkit-patch.md`); it installs to `~/.local/webkitgtk-patched`.

```bash
./tools/build-native.sh
./tools/run-native.sh
node tools/enter.mjs
./tools/stop.sh
```

Both shells share one store and one port; the run scripts refuse to start one
while the other is up.

**Alt+Enter** toggles fullscreen in both shells. The native shell catches the
key before the page and fullscreens the window, so pointer lock survives;
the wry shell takes the page's message and toggles the window itself.

Mouse look is scaled to match the desktop game: Wayland hands WebKit raw
relative motion, while the Electron game on X11 sees the desktop's pointer
acceleration. The default gain is 2×; `DFO_MOUSE_GAIN` overrides it (`1` keeps
the raw deltas).

## Measured

Test machine: Ryzen 7 5700X3D, Radeon RX 6750 XT, Mesa 26.2.4, Plasma 6 /
Wayland.

| | `wry` / GTK3 | native GTK4 |
|---|---|---|
| engine | WebKitGTK 2.54.1 | WebKitGTK 2.54.2, patched |
| display | 1920×1080 @ 144 Hz | 2560×1440 @ 165 Hz |
| Sentinel, 64 blocks / 568 draws | 59 fps, 17 ms | **165 fps, 6.1 ms** |
| Test Room | 60 fps, 17 ms | 144–154 fps, 6.5–7.0 ms |

The Test Room is the slower scene on the GTK4 shell because it draws more
(948 draws against Sentinel's 568); the engine is the constraint, not the
display.

| measurement | result |
|---|---|
| first paint | ~0.7 s |
| world ready (Test Room) | ~26 s |
| 3D graphics (WebGL2) | yes |
| saves and settings | 96 real entries; Load Game lists the real characters |
| memory in world | game process ~2.5 GB, network ~90 MB |
| content size | 1280×800 |

These numbers assume hardware graphics. A machine that falls back to software
rendering will not hold 60 fps.

## Known gaps / next steps

1. **Single instance.** The fixed port already refuses a second shell; make it
   a real lock that raises the window instead.
2. **The launcher.** `game/launcher/` is extracted and unchanged; its bridge is
   two words (`onView`, `act`), easy to port once the game window is settled.
3. **ARENA2 detection and the folder picker** (Steam/GOG/DFU settings search) —
   the official shell's `arena2Detect.cjs` is the reference.
4. **Packaging the patched WebKit** (~150 MB). Until the patch is upstream, the
   native shell needs a private build on each machine.
5. **Display selection.** Frame timing follows the main display rather than the
   window's output: fine for play there, but a second patch would be needed for
   other outputs.
6. **Mod loader integration.** The mods patch the served bytes; a shell that
   serves `game/dist` can apply the same view/overlay before responding. The
   control channel is a placeholder for the `dfo` CDP tooling on Linux, which
   does not exist for WebKitGTK.

## Licensing

The game source and shell are MIT (Daggerfall Unity's notice included there).
ARENA2 belongs to the player and is never bundled; this repository ignores
`game/` and `vendor/` for that reason.
