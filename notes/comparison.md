# The like-for-like comparison

Both shells ran the same build, the same store, the same scene, the same
content size and the same output.

- **Build**: 0.1.14473 (the launcher updated the AppImage when the online
  session quit; the Rust shell was re-extracted from that AppImage).
- **Machine**: AMD Ryzen 7 5700X3D (8C/16T), Radeon RX 6750 XT (`radeonsi`,
  Mesa 26.2.4), Plasma 6/KWin 6.7.5 Wayland; DP-1 is 1920×1080 @ 144 Hz.
- **Store**: both read and write `~/.config/Daggerfall JavaScript/Prefs` and
  `Saves`. Same settings, same keybinds, same saves.
- **Scene**: Offline → Test Room → "Enter as the Nord Warrior", the opening
  cave narration on screen, HUD up, not moving.
- **Window**: 1280×772 content (the Electron window's menu bar takes 28 px),
  on **DP-1**, the leftmost output (1920×1080 @ 144 Hz, geometry 0,360).
- **Sample**: 3 s of `requestAnimationFrame` after the world settled (~45–60 s
  after entry), focused.

## Results

| | Electron 42.11.8 (Chromium 148) | Rust + WebKitGTK 2.54.1 |
|---|---|---|
| frame rate | **144 fps** (follows the 144 Hz output) | **60 fps** (capped) |
| median frame | 6.9 ms | 17 ms |
| p95 frame | 7.3 ms | 18 ms |
| worst frame | 7.8 ms | 18 ms |
| total RSS | **2.9 GB** | **3.64 GB** |
| shell process | ~370 MB main + 55 MB GPU + … | 163 MB |
| network process | ~72 MB | 83 MB |
| web/renderer | (renderer included in total) | 3.40 GB |
| GL adapter | `ANGLE (AMD, Radeon RX 6750 XT, OpenGL ES 3.2)` | reported as `Apple GPU` (WebKitGTK sanitizes); system GL is `radeonsi` |
| window | 1280×828 outer, DP-1 | 1280×800 outer, DP-1 |

## The two findings that matter

1. **WebKitGTK caps `requestAnimationFrame` at 60 Hz on this machine.** The
   same page, same settings, same 144 Hz output: Chromium delivers 144, WebKitGTK
   delivers 60. It is not the page's frame cap (`framesLifted` is false in both,
   so the page is not holding frames) and not focus (measured focused). This is
   the one hard performance loss of the Linux webview route; Electron's 60-fps
   default would hide it, a 144 Hz player will not miss it.
2. **There is no memory win.** Both shells use the same order of memory, and in
   this scene WebKitGTK used ~750 MB more. The game's own heap dominates; the
   shell's binary size does not translate into RSS. (Electron sampled 4.5 GB
   during world generation and settled to 2.9 GB, so treat ±0.5 GB as noise.)

The wins that remain are real but narrower than they look on disk: ~200 MB less
to ship (the 220 MB Electron runtime becomes a ~15 MB binary plus the system
WebKitGTK), no bundled Chromium to keep patched, and a shell written in Rust.
The costs are the 60 Hz cap, the shell parity work, and losing CDP tooling.

## How the runs were driven

Rust: `./tools/run.sh --width 1280 --height 772`, then `node tools/enter.mjs`,
then the same rAF expression through `tools/ctl.mjs`.

Electron: the installed AppImage (`DAGGER_NO_UPDATE_CHECK=1`, no
`DAGGER_DEV_URL`, so stock, no mods) with `--no-sandbox
--remote-debugging-port=9222`. `dfo enter` presses Play and enters the Test
Room, but its final wait is on `window.__playerEntity`, which the stock build
deletes (the mods' skyhook keeps it), so it times out after entering — the
world is up. The window is native Wayland, so `dfo place`/xdotool cannot see
it; KWin's `sendClientToScreen` placed it on DP-1, and a KWin script focused it.

## Window placement

- Electron launcher and game windows are class `dfo-online`, caption
  `DFO Online` (the game's title is set by `main.cjs`, not the page).
- `workspace.sendClientToScreen(w, output)` works for both. It centres the
  window; there is no scriptable absolute move on Wayland.
- The Rust shell does this itself (`src/placement.rs`, `DFO_WINDOW`).
