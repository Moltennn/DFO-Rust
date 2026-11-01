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
src/lib.rs         shared: origin, store, probe, state dir
src/main.rs        GTK4 window and app
src/server.rs      local origin: dist/, ARENA2, /__dfo, /__store
src/store.rs       file store (Prefs/ + Saves/, the Electron shell's layout)
src/probe.js       injected probe + daggerShell storage bridge
src/webkit_shim.c  the WebKitGTK 6.0 calls with no usable Rust binding
build.rs           pkg-config + the shim; warns on the system WebKit
tools/             extract-build.mjs, build.sh, run.sh, stop.sh, ctl.mjs,
                   enter.mjs, state.mjs (the one definition of the state dir)
notes/             experiments 1-2 and the native spike
```

Local state and tooling, **outside the repo and outside `/tmp`**:

```
${DFO_STATE_DIR:-$HOME/.local/share/dfo-rust}/
  url                 the running shell's base URL (ctl/enter read this)
  shell.pid           run.sh / stop.sh
  shell.log           the shell's stderr/stdout
  backups/store-2026-11-01/    the player's Prefs/ + Saves/ copy taken before
                               the first bridge run (restore by copying back)
  kwin/               transient KWin scripts used to place/focus the Electron
                      and native windows during comparisons
  logs/2026-11-01/    screenshots and logs of the comparison
  webkit-patched/     native spike: WebKitGTK 2.54.2 source, the display-link
                      patch, build.sh (private prefix) and run-probe.sh
```

`DFO_STATE_DIR` overrides the directory; `XDG_DATA_HOME` is respected.

The shell links a **private** patched WebKitGTK 6.0, never the system one:
`~/.local/webkitgtk-patched` (`DFO_WEBKIT_PREFIX` overrides; the source,
patch and scripts are in `$STATE/webkit-patched/`). `tools/build.sh` points
pkg-config at it; `tools/run.sh` sets `LD_LIBRARY_PATH`. A plain `cargo build`
warns because it picks the system library and runs at 60 fps
(`DFO_ALLOW_SYSTEM_WEBKIT=1` silences it).

`game/` (the extracted build) and `vendor/` (an asar extraction) are
git-ignored. Never commit game data, ARENA2, saves or screenshots of the
player's data to the repository.

## Run

Requirements: Rust, GTK4, and the private patched WebKitGTK 6.0 (built once,
`notes/native-webkit-patch.md`). The player's ARENA2 path comes from
`~/.config/Daggerfall JavaScript/config.json` (`arena2Path`) or `--arena2`.

```bash
# one-time: pull the current build out of the installed AppImage
mkdir -p vendor && (cd vendor && /path/DFO-Online-linux-x86_64.AppImage \
  --appimage-extract 'resources/app.asar')
node tools/extract-build.mjs          # -> game/dist + game/launcher
rm -rf vendor

./tools/build.sh                      # cargo build, private WebKit prefix
./tools/run.sh                        # 1280x800 window, server on :47565
node tools/enter.mjs                  # intro -> Offline -> Test Room -> character
node tools/ctl.mjs 'document.title'   # any JS expression, printed
./tools/stop.sh
```

`tools/run.sh` passes arguments to the shell:

- `--width N --height N` — content size (the comparison used 1280×772)
- `--serve-only` — the origin without a window, for pointing a browser at it
- `--url URL` — load an existing page instead of starting the origin
- `--root DIR`, `--arena2 DIR`, `--port N`

The port is pinned to 47565 because WebKitGTK keys `localStorage` and
IndexedDB by origin, and an origin includes the port.

The window opens on the active KWin output; WebKit's frame timing follows the
main display (DP-3 here), so that is the output where the 164 fps numbers come
from. The near-60 feature is always disabled in the C shim (6.0 has no
`webkit_web_view_new_with_settings`, so the settings are applied to a fresh
view before the first load).

The shim allows WebKit's **pointer-lock** permission request
(`permission-request`). WebKitGTK keeps the request pending if the app does not
decide: the cursor stays visible and the game, which only looks around under
pointer lock, falls back to its slow unlocked path.

The probe makes pointer lock reliable and the mouse feel like the desktop game:

- on the first `mousedown` on the game canvas it requests pointer lock itself,
  synchronously in the gesture; the game's unadjusted request fails and its
  promise retry can miss the gesture (the first click used to leave the game
  unlocked);
- while locked, it forces `cursor: none !important` over the game's custom
  cursor images;
- it scales locked `movementX`/`movementY` by `DFO_MOUSE_GAIN` (default 2.0),
  because Wayland relative motion is raw while Electron on X11 sees the
  compositor's pointer acceleration.

**Alt+Enter** fullscreens the window. GTK4 handles it with a capture-phase key
controller, so the page never sees it and pointer lock stays.

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
GTK4 with WebKitGTK 6.0 has a 144 Hz frame clock and the near-60 feature off,
yet the page still measures ~62 fps (`tools/gtk4-raf.py`).

The cause is WebKitGTK's frame timing. It takes the rate from a DRM vblank
monitor; that monitor is unavailable to a Wayland client that is not the DRM
master, so it falls back to `DisplayVBlankMonitorTimer`, hard-coded to 60. The
giveaway: `WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=144` is rejected with "not a
factor of refresh rate 60fps" while the GTK clock is 144. A single-output run
still reports 60, so it is not the monitor layout, and VSync / compositing
settings do not change it.

The fix is a small patch to that fallback: pass the monitor's refresh rate to
the timer. `notes/native-webkit-patch.md` has the patch, the private build
(`~/.local/webkitgtk-patched`) and the caveats. With it, the shell runs
Sentinel at **164 fps, mean 6.1 ms** on DP-3.

The like-for-like numbers (Sentinel, 1280×772, DP-3 @ 165, mean of three 5 s
runs): official Electron **165 fps / 6.06 ms**, this shell **164 / 6.1 ms**.

One caveat remains: the page's frame timing follows the first display it sees
(DP-3, the main display here), not the window's output, and moving the window
does not repoint it. Fine for play on DP-3; another patch would be needed for
other outputs. The fallback is also a timer, not vsync-locked, so tearing is
possible even at the right rate.

## Comparing against Electron

Both read one store (do not run them against it at once) and the same build.
The current build is the installed AppImage's `app.asar` (0.1.14473 at the time
of writing); re-extract after a launcher update.

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
