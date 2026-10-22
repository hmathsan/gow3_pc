# launcher-egui (spike)

The launcher's shell and Home page on [egui](https://github.com/emilk/egui) +
[fastframe](https://github.com/crmne/fastframe), built on the same `core` as the iced launcher in
`../launcher-rust` (with its `iced-ui` feature off). It exists to compare the two toolkits on
the same machine, not to ship.

```bash
cd launcher-egui && cargo run --release
```

`SPARTAN_PORT_DIR` / `SPARTAN_DATA_DIR` work as for the iced launcher. The spike only reads files:
nothing is saved.

## Ported

- Window, icon, minimum size, the startup steps (`startup::prepare`), `--version` and the Python
  routes.
- Home: the five background layers, key art, heat pulse and fire (same simulation, drawn like the
  iced canvas fire), monogram, logotype, PLAY, chips, settings line, status line, version and
  language buttons. The language list switches the window live.
- Tab bar with the nine tabs, the arrow keys and Esc, and the drawer sliding in at its page's
  width with the dim layer behind it. Everything mirrors right to left.

## Not ported

The drawer pages, splash, first-run setup, version menu, modals, PLAY/STOP, saving, the GPU
probe, GUI Scale and the snapshot tests.

## Measured

Both release builds were run on the same machine (Windows 11, Ryzen 7 5700X3D, RTX 5070), Home
page, a settings file with no game folder, and no update check. Each figure is the median of 5
runs after one discarded warm-up launch per exe. CPU was sampled over 10 s after 8 s of settling,
with the window confirmed in the foreground (focus changes the fire's intensity in both):

| | iced (#959) | egui spike |
|---|---|---|
| exe size | 24.6 MB | 13.7 MB |
| start to window | ~550 ms | ~55 ms |
| working set | ~191 MB | ~103 MB |
| private bytes | ~359 MB | ~176 MB |
| CPU, fire on (the default) | 3.0 % (2.8–3.6) | 5.5 % (4.1–7.2) |
| CPU, fire off | 1.9 % (0.2–2.7) | 0.0 % |

Read with care:

- The fire (`launcher_effects`) is on by default, so the fire-on row is what players get, and
  there the spike costs more CPU than iced.

- The iced exe carries every page and the egui one only the shell and Home. Most of the size
  gap is the toolkit (wgpu against glow), but not all of it.
- "Start to window" is the time until the OS window exists, not the first painted frame. The iced
  launcher also probes for a wgpu adapter before it opens its window.
- With the fire on, both repaint at 30 fps, and iced's frame costs less than eframe+glow's here:
  drawing nothing but the fire, or switching vsync off, didn't change the spike's number. eframe's
  wgpu renderer was not tried. With the fire off, egui doesn't repaint at all, while the iced
  launcher keeps a 100 ms clock.

## What it depends on

- `crmne/egui` at the revision Spotifast pins (`[patch.crates-io]` in `Cargo.toml`): stock egui
  0.36 does not shape Arabic. The fork shapes right-to-left runs (emilk/egui#8577, still open
  upstream).
- `src/bidi.rs`, vendored from Spotifast (MIT, `LICENSE-spotifast`): the fork still places runs in logical order, so
  lines that mix Arabic and Latin text (`Insert أو R3+L2`) come out scrambled without it. This
  code lives in Spotifast itself, not in egui or fastframe.
- `fastframe-fonts` (the system UI face plus per-script system fallbacks: Arabic, CJK) and
  `fastframe-text` (hinting that follows the desktop). Both are git-only (tag `v0.14.0`).
- Rust 1.98 (`rust-toolchain.toml`), as fastframe requires.
