# DFO Rust Shell

> **Proof of concept — use at your own risk.** This is an experiment, not a
> supported client. It writes to the real `Daggerfall JavaScript/Prefs` and
> `Saves` folders. Back them up first. Not for online play.

An experimental, no-Electron desktop shell for **DFO Online**, the open-source
(Daggerfall-Unity-derived, MIT) web game. The game is unchanged; only the
`app/` shell is replaced, starting with offline play on Linux.

DFO Online is unofficial and not affiliated with Bethesda Softworks or ZeniMax
Media. See [NOTICE](NOTICE) for attribution, the upstream licences and the
full disclaimer. The shell's own code is MIT (see [LICENSE](LICENSE)).

## Status

A spike: a window, a local origin, the player's ARENA2 from disk, and a
file-backed store. The launcher, updater and mod-loader integration are not
here yet, and the Linux engine has a known 60 fps cap under investigation.

## Layout

```
src/        the shell (Rust)
src/probe.js  the page-side probe, injected at document start
tools/      build extraction, run/stop, page control
game/       extracted build (git-ignored; never committed)
```

Details of each experiment are in `notes/` as they are written.
