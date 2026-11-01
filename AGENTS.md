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
notes/             experiments 1-3, the Electron comparison, the native spike
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
  kwin/               transient KWin scripts used to place/focus Electron and
                      Vivaldi windows during comparisons
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

GTK3's frame clock is 16667 µs (59 Hz) in every configuration tried. The
WebKit `PreferPageRenderingUpdatesNear60FPS` feature and `GDK_GL=disable`
(each, and together), XWayland, and disabling compositing do not move it. The
same page in Chromium (Vivaldi app-mode) is 144 fps. `notes/experiment-3.md`
has the matrix.

**GTK4 + WebKitGTK 6.0 with the stock library was tested too** (2.54.2,
`tools/gtk4-raf.py`): the GTK4 frame clock is 6944 µs (144 Hz), the near-60
feature is off at web-process construction, the window is focused and visible,
and rAF is still ~62 fps / 16 ms. Setting GTK4 aside, the cap is WebKitGTK's
GLib display-link timer fallback, which is what the native patch replaces.

Output refresh and VSync are not the cause either: 144 Hz (DP-1) and 165 Hz
(DP-3) outputs both give ~62 fps while the GTK clock is 6944/6067 µs;
`vblank_mode=0`, XWayland and `WEBKIT_DISABLE_COMPOSITING_MODE=1` are all ~62.
Chromium with `--disable-gpu-vsync --disable-frame-rate-limit` reaches
**243 fps** in the same Sentinel scene. The remaining non-native options are
Chromium app-mode (144, but a browser dependency) or CEF (bundled Chromium);
the native option is the patched WebKit below.

The root of it: WebKitGTK's GLib `DisplayLink` takes its rate from a DRM
vblank monitor; that monitor is unavailable here, so it falls back to
`DisplayVBlankMonitorTimer`, hard-coded to 60 (`WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=72`
is rejected with "refresh rate 60fps" while the GTK clock is 144). A patch to
the fallback (use the monitor's refresh) plus a GTK4 shell is the native fix;
it needs a WebKitGTK rebuild.

The native fix is built and works: `notes/native-webkit-patch.md` has the
patch, the private build and the second issue. The patched WebKitGTK 2.54.2
(GTK4, `~/.local/webkitgtk-patched`) makes `DisplayVBlankMonitorTimer` take the
monitor's refresh, and `gtk4-shell/` runs the game on it: on DP-3 / 165 Hz,
Sentinel (64 blocks / 568 draws) holds **165 fps, mean 6.07 ms**, with a
6067 µs GTK4 frame clock.

One caveat remains: the page's display link follows the display it first sees
(DP-3, the main display) rather than the window's output. A window on DP-1
still gets the 165 Hz link while its GTK clock is 144, and moving the window
does not repoint it. That is acceptable while the game is played on DP-3; a
second display-link selection patch would be needed otherwise. The fallback is
also a timer, not vblank-synced, so tearing/jitter is possible even at the
right rate.

## Comparing against Electron and Chromium

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

Chromium app-mode against the Rust origin:

```bash
./tools/run.sh --serve-only
vivaldi-stable --user-data-dir="$HOME/.cache/dfo-rust/vivaldi" \
  --no-first-run --password-store=basic --remote-debugging-port=9222 \
  --window-size=1280,772 \
  --app='http://127.0.0.1:47565/play/index.html?exterior&region=Sentinel&loc=Sentinel&shot&class=0'
```

Place/focus windows on the leftmost output with the scripts in
`$STATE/kwin/` (classified by window class: `dfo-online`, `dfo-shell`,
`vivaldi-127.0.0.1__play_index`), loaded through
`org.kde.KWin.Scripting` (`qdbus6 ... loadScript <file> <name>` then `start`).
KWin's scriptable `move`/`moveResize` are booleans, not methods;
`workspace.sendClientToScreen(window, output)` is what works. Wayland clients
cannot position themselves.

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
