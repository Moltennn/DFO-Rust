# Experiment 2 — the storage bridge, and window placement

Date: 2026-11-01. Same build/settings as experiment 1.

## The bridge

What the page sees is the Electron preload's contract, backed by the same
files:

- `GET /__store/all` seeds an in-memory map from the page's initialization
  script. **Synchronous XHR works at document start in WebKitGTK** — this was
  the open question; it does, so reads are in-memory and never wait on IPC.
- `setItem` / `removeItem` use a synchronous XHR too, so the caller's
  try/catch still sees a disk failure (the game treats a throw as quota).
- `src/store.rs` writes `Prefs/<key>` (the shell's percent-encoded names),
  `Saves/SAVE<n>/SaveData.txt`, `SaveInfo.txt`, `Screenshot.jpg` (data URLs
  decoded; unknown MIMEs kept verbatim as `.dataurl`), temp-then-rename,
  empty slot folders swept.
- `length()` rescans `/__store/keys` every 2 s, as Electron's store does, so a
  save copied into the folder appears without a relaunch.

Root: `DAGGER_USER_DATA`, else `$XDG_CONFIG_HOME/Daggerfall JavaScript` — the
same store the Electron shell uses.

## Verified

- Bridge mode reported `files`, **96 keys** from the real store.
- The game booted with the player's own settings: `Video.FieldOfView` 90 read
  from `dagger.settings.v1`, and the intro skipped (the player's settings knew
  it had been seen).
- **Load Game lists the real saved characters** — "Warrior / QUICKSAVE", the
  level-30 Dark Elf, health, gold and dates — parsed from `SaveInfo.txt` and
  `SaveData.txt` through the bridge.
- The game **wrote** during the session: `Prefs/dagger.ui.v1` at 05:12 and
  `Prefs/dagger.revenant.c5a4…` at 05:11, i.e. the write path is exercised by
  the game itself, not only by a synthetic `setItem`.
- A synthetic write/read/remove round trip landed in `Prefs/` and disappeared
  from it.
- Entering the Test Room: 60 fps after warm-up (median 17 ms, p95 19 ms, worst
  25 ms); the first seconds showed a 409 ms hitch while world generation
  finished, which the 3 s sample averages down.

The user's `Prefs/` and `Saves/` were copied to
`~/.local/share/dfo-rust/backups/store-2026-11-01/` before the first bridge run (the
shell's state dir, `DFO_STATE_DIR` overrides); both shells now
read and write the same files.

## Window placement (Wayland, KWin 6.7.5)

- Wayland clients cannot position themselves, and a GTK/Wayland window is
  invisible to X11 tools (`xwininfo` shows nothing; `xdotool` cannot touch it).
- KWin's scripting `Window` object exposes `move` and `resize` as **booleans**
  (the moveable/resizeable flags), not methods; `x`, `y`, `pos` and
  `frameGeometry` are read-only. `moveResize` does not exist on
  `XdgToplevelWindow`.
- What works: `workspace.sendClientToScreen(window, output)` — it moves the
  window to that output, centred. `workspace.screens` gives `{name, geometry}`
  (DP-1 0,360 1920×1080; DP-3 1920,0 2560×1440; DP-2 4480,360 1920×1080).
- `src/placement.rs` writes a small script (find `dfo-shell`, target the named
  output or the leftmost, retry for 10 s), loads it via
  `org.kde.KWin.Scripting`, starts it, unloads it after 12 s.
- Result: the shell opens at **320,471, 1280×800 content on DP-1** — centred
  on the leftmost output, exactly where `dfo place DP-1` puts the Electron
  window.
- KWin script `print()` goes to the user journal
  (`journalctl --user -b | grep "DFO place"`), which is how the API was
  probed.

## Left for the comparison

- Same Test Room, same settings, same window size, under Electron, measuring
  FPS and total RSS.
- A city/dungeon scene, where sprite and lighting work dominates.
