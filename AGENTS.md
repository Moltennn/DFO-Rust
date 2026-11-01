# AGENTS.md — DFO Rust Shell

An experimental, no-Electron desktop shell for **DFO Online** (the open-source,
Daggerfall-Unity-derived web game). Status: spike, offline only, proof of
concept. The game is unchanged; this replaces only the Electron `app/` shell.
See `README.md` for the overview and `NOTICE` for attribution and the full
disclaimer.

This is a separate project from the modding workspace at `../DFO-Mods`. The
game's own `dfo` tooling (`~/.local/share/dfo-tools`) drives the **Electron**
game over CDP; this shell has no CDP and uses its own control channel.

## Where things live

Repository:

```
src/lib.rs         shared: origin, store, placement, probe, state dir
src/main.rs        wry/GTK3 window (default "wry-shell" feature)
src/server.rs      local origin: dist/, ARENA2, /__dfo, /__store
src/store.rs       file store (Prefs/ + Saves/, the Electron shell's layout)
src/placement.rs   KWin output placement (sendClientToScreen)
src/probe.js       injected probe + daggerShell storage bridge
gtk4-shell/        native GTK4 + WebKitGTK 6.0 binary and its C shim
tools/             extract-build.mjs, run.sh, run-native.sh, stop.sh, ctl.mjs,
                   enter.mjs, build-native.sh, state.mjs (state dir)
notes/             experiments 1-2 and the native spike
```

Local state and tooling, **outside the repo and outside `/tmp`**:

```
${DFO_STATE_DIR:-$HOME/.local/share/dfo-rust}/
  url                 the running shell's base URL (ctl/enter read this)
  shell.pid           run.sh / stop.sh
  shell.log           the shell's stderr/stdout
  shell-native.pid    run-native.sh / stop.sh
  shell-native.log    the GTK4 shell's stderr/stdout
  backups/store-2026-11-01/    the player's Prefs/ + Saves/ copy taken before
                               the first bridge run (restore by copying back)
  kwin/               transient KWin scripts used to place/focus the Electron
                      and native windows during comparisons
  logs/2026-11-01/    screenshots and logs of the comparison
  webkit-patched/     native spike: WebKitGTK 2.54.2 source, the display-link
                      patch, build.sh (private prefix) and run-probe.sh
```

`DFO_STATE_DIR` overrides the directory; `XDG_DATA_HOME` is respected.
`/tmp` is only used by `src/placement.rs` for the KWin script it writes,
loads and removes.

The native GTK4 shell links a **private** patched WebKitGTK 6.0, never the
system one: `~/.local/webkitgtk-patched` (`DFO_WEBKIT_PREFIX` overrides; the
source, patch and scripts are in `$STATE/webkit-patched/`). The wry binary
keeps using the system `webkit2gtk-4.1`; never link both GTK versions into one
process.

`game/` (the extracted build) and `vendor/` (an asar extraction) are
git-ignored. Never commit game data, ARENA2, saves or screenshots of the
player's data to the repository.

## Run

Requirements: Rust, `webkit2gtk-4.1`, GTK3. The player's ARENA2 path comes from
`~/.config/Daggerfall JavaScript/config.json` (`arena2Path`) or `--arena2`.

```bash
# one-time: pull the current build out of the installed AppImage
mkdir -p vendor && (cd vendor && /path/DFO-Online-linux-x86_64.AppImage \
  --appimage-extract 'resources/app.asar')
node tools/extract-build.mjs          # -> game/dist + game/launcher
rm -rf vendor

cargo build
./tools/run.sh                        # 1280x800 window, server on :47565
node tools/enter.mjs                  # intro -> Offline -> Test Room -> character
node tools/ctl.mjs 'document.title'   # any JS expression, printed
./tools/stop.sh
```

`tools/run.sh` passes arguments to the shell:

- `--width N --height N` — content size (the comparison used 1280×772)
- `--no-near-60-fps` — turn off WebKit's near-60 preference (a knob; it does
  not lift the cap on GTK3)
- `--serve-only` — the origin without a window, for pointing a browser at it
- `--url URL` — load an existing page instead of starting the origin
- `--root DIR`, `--arena2 DIR`, `--port N`

The port is pinned to 47565 because WebKitGTK keys `localStorage` and
IndexedDB by origin, and an origin includes the port.

### Native GTK4 route

`gtk4-shell` is the second binary: same game, same origin, same store, but a
GTK4 window on WebKitGTK 6.0 from the private patched WebKit. It exists because
GTK3 cannot go past 59 Hz; this one holds the monitor's rate.

```bash
./tools/build-native.sh   # PKG_CONFIG_PATH -> ~/.local/webkitgtk-patched
./tools/run-native.sh     # LD_LIBRARY_PATH  -> same prefix
node tools/enter.mjs
node tools/ctl.mjs 'document.title'
./tools/stop.sh
```

It accepts the same flags (`--serve-only`, `--url`, `--root`, `--arena2`,
`--port`, `--width/--height`). The near-60 feature is always disabled in the
C shim (6.0 has no `webkit_web_view_new_with_settings`, so the settings are
applied to a fresh view before the first load).

Both shells write `$STATE/url`, and `run.sh` / `run-native.sh` refuse to start
while the other is running: one store, one origin. The window opens on the
active KWin output; WebKit's display link follows the main display (DP-3 here),
so that is the output where the 165 fps numbers come from.

## Driving the page

`tools/ctl.mjs` queues one JS expression; the injected probe evals it and posts
the answer. `tools/enter.mjs` walks the menu by visible text. Neither touches
the mouse. The state URL is in the state dir above.

The game's own scene router is the teleport for locations:

```
/play/index.html?exterior&region=Sentinel&loc=Sentinel&shot&class=0
```

- `?exterior&region=<region>&loc=<loc>` assembles a location from MAPS.BSA
  (bare `?exterior` is Daggerfall city). Region/loc are MAPS.BSA names.
- `?shot` exposes the debug API: `__playerEntity`, `__player` (`pos`, `eye`,
  `warp`), `__move`, `__pose`, `__console`, `__fpsStats`, `__renderer`,
  `__foes`, `__automapCommand`, and more. It bypasses the menu for dungeons.
- `?class=N` is the headless chargen skip (N is a CLASS*.CFG index); without it
  a chargen overlay covers the loaded scene.
- `?test=<preset>` seeds the world host's Test Room character (the menu's
  Offline → Test Room door sets it).
- Imperial City is **not** a MAPS.BSA location; `?exterior&region=Cyrodiil&
  loc=Imperial City` fails. In-game travel there is 882 in-game hours and can
  kill a level-1 character. Sentinel is the heaviest ordinary MAPS.BSA city.

## The 60 Hz cap

The stock engine limits the page to about 60 fps, independent of the display.
GTK3's frame clock is 16667 µs (59 Hz); GTK4 with WebKitGTK 6.0 has a 144 Hz
clock and the near-60 feature off, yet the page still measures ~62 fps
(`tools/gtk4-raf.py`).

The cause is WebKitGTK's frame timing. It takes the rate from a DRM vblank
monitor; that monitor is unavailable to a Wayland client that is not the DRM
master, so it falls back to `DisplayVBlankMonitorTimer`, hard-coded to 60. The
giveaway: `WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=144` is rejected with "not a
factor of refresh rate 60fps" while the GTK clock is 144. A single-output run
still reports 60, so it is not the monitor layout, and VSync / compositing
settings do not change it.

The fix is a small patch to that fallback: pass the monitor's refresh rate to
the timer. `notes/native-webkit-patch.md` has the patch, the private build
(`~/.local/webkitgtk-patched`) and the caveats. With it, `gtk4-shell/` runs
Sentinel at **165 fps, mean 6.07 ms** on DP-3.

One caveat remains: the page's frame timing follows the first display it sees
(DP-3, the main display here), not the window's output, and moving the window
does not repoint it. Fine for play on DP-3; another patch would be needed for
other outputs. The fallback is also a timer, not vsync-locked, so tearing is
possible even at the right rate.

## Comparing against Electron

Both need a free store (do not run two shells against it at once) and the same
build. The current build is the installed AppImage's `app.asar` (0.1.14473 at
the time of writing); re-extract after a launcher update.

Electron (stock, no mods, no `DAGGER_DEV_URL`):

```bash
cd ../DFO-Mods
DAGGER_NO_UPDATE_CHECK=1 ./DFO-Online-linux-x86_64.AppImage \
  --no-sandbox --remote-debugging-port=9222
dfo enter                 # it times out on __playerEntity (stock has no mods);
                          # the world is loaded anyway
dfo eval '<rAF expression>'
```

Place/focus it on an output with the scripts in `$STATE/kwin/` (window class
`dfo-online`), loaded through `org.kde.KWin.Scripting` (`qdbus6 ... loadScript
<file> <name>` then `start`). KWin's scriptable `move`/`moveResize` are
booleans, not methods; `workspace.sendClientToScreen(window, output)` is what
works. Wayland clients cannot position themselves.

## Data safety

The shell writes the real `~/.config/Daggerfall JavaScript/Prefs` and `Saves`
through the file store. Back them up before changing store code (the existing
copy is in `$STATE/backups/`). Unit tests in `src/store.rs` use temp dirs and
cover the pref-name encoding, the canonical slot gate and a disk round trip.

## Commits

One capability per commit; tests with the code they cover; docs either in that
commit or a separate `docs:` commit; no feature+refactor mixes. Prefixes:
`feat(area):`, `fix(area):`, `docs:`, `chore:`. The history was rewritten once
before the first push to keep this granular. MIT + `NOTICE`; no game data.
