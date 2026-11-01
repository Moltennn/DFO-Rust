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
src/main.rs        window, probe injection, state dir, --no-near-60-fps
src/server.rs      local origin: dist/, ARENA2, /__dfo, /__store
src/store.rs       file store (Prefs/ + Saves/, the Electron shell's layout)
src/placement.rs   KWin output placement (sendClientToScreen)
src/probe.js       injected probe + daggerShell storage bridge
tools/             extract-build.mjs, run.sh, stop.sh, ctl.mjs, enter.mjs,
                   state.mjs (the one definition of the state dir)
notes/             experiments 1-3 and the Electron comparison
```

Local state and tooling, **outside the repo and outside `/tmp`**:

```
${DFO_STATE_DIR:-$HOME/.local/share/dfo-rust}/
  url                 the running shell's base URL (ctl/enter read this)
  shell.pid           run.sh / stop.sh
  shell.log           the shell's stderr/stdout
  backups/store-2026-11-01/    the player's Prefs/ + Saves/ copy taken before
                               the first bridge run (restore by copying back)
  kwin/               transient KWin scripts used to place/focus Electron and
                      Vivaldi windows during comparisons
  logs/2026-11-01/    screenshots and logs of the comparison
```

`DFO_STATE_DIR` overrides the directory; `XDG_DATA_HOME` is respected.
`/tmp` is only used by `src/placement.rs` for the KWin script it writes,
loads and removes.

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

The remaining native route is **GTK4 + WebKitGTK 6.0** (GTK4's frame clock
requests Wayland callbacks). wry/tao are GTK3-only; the `webkitgtk` Rust crate
is an unpublished placeholder. `webkitgtk-6.0` 2.54.2 is in Arch `extra`, but
this machine needs a full `pacman -Syu` first — never a partial upgrade. Test
it with a small GTK4/WebKitGTK 6.0 page (Python GI or a C shim) before porting
the shell.

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
