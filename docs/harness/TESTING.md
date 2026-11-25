# Testing

## Gate

```powershell
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo test
```

Run all three before every code commit. `cargo test` is a debug build: egui/epaint debug
asserts only fire there, so a release-only check is not enough.

## What to test, and how

- Bugs start with a failing test that reproduces the report. Run it red, then fix.
- Keep logic in pure functions and test those: layout maths (`sidebar::row_layout`,
  `files::pane_split`, `app::strip_slots`, `ui_theme::cell_size`, `resize::hold_frame`),
  parsers (`osc`, `gitinfo::parse_head/parse_porcelain_z/parse_numstat_z`,
  `changes::parse_unified/compare_url`), decisions (`status::next_status`,
  `dir_picker::visible_rows/step`, `window_watch::changes/layout_untrusted`,
  `procs::has_foreground_child`, `layout::new_session_cwd`, `config::migrate`).
- Panels that are hard to click headlessly (changes pane, inbox): render a few frames with
  `run_ui` and fake state so egui's debug asserts (id clashes, layout) still run.
- Terminal behaviour: drive `alacritty_terminal` directly with `ansi::Processor` on a `Term`
  and an mpsc event proxy; assert on the events and replies (see backend.rs tests).
- Painting: build shapes with the same helpers `paint_grid` uses and assert on rects and
  colours (see view.rs tests); compare in device pixels.
- File system: create a unique dir under `std::env::temp_dir()` (include the process id) and
  remove it at the end.

## Headless egui

```rust
let ctx = egui::Context::default();
let raw = egui::RawInput {
    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 700.0))),
    events,
    ..Default::default()
};
let mut out = ctx.run_ui(raw, |ui| { /* draw the widget */ });
out.textures_delta.clear(); // dropping pending deltas panics in debug
```

- A click is three frames: `PointerMoved(pos)`, then `PointerButton { pressed: true }`, then
  `PointerButton { pressed: false }` (see `sidebar::tests::click`).
- Keys: `Event::Key { key, physical_key, pressed, repeat, modifiers }`; check the event was
  consumed (`ui.input(|i| i.events.len())`) so the terminal never sees a shortcut.

## Ignored tests

- `conpty_passes_the_osc_11_background_query_through` spawns a real ConPTY:
  `cargo test conpty_passes -- --ignored`. Green with the vendored dll that build.rs copies;
  red on the inbox ConPTY.
- `real_repo_changes_and_diffs_load` runs git on this checkout and checks every diff's added
  lines match numstat: `cargo test real_repo -- --ignored --nocapture`.

## What tests cannot show

Pixels on the real screen, focus between windows, other apps' hotkeys, a TUI's real redraw.
Use these, and say in the report what was not verified by eye:

- The log: `%APPDATA%\ankar\ankar-log.txt`. Set `log_level = "debug"` in config.toml (or
  RUST_LOG) for PTY queries/replies; `ankar=trace` adds every key pressed with Ctrl.
- Palette → "Debug: check colors (log)": logs painted vs read-back pixels.
- `docs/ram.md` for memory, read with `Get-Process ankar`.

## Tooling gotchas

- A running `ankar.exe` locks `target\release\ankar.exe` ("Access is denied" on build).
  Rename the old exe (a running exe can be renamed, not overwritten) or close Ankar.
- Editor writes can reach the disk a few seconds late: if the compiler reports code you
  already changed, wait and run again before debugging.
- `examples/` holds local scratch programs; they still compile under `--all-targets`.
- The agent sandbox sets `CARGO_TARGET_DIR` elsewhere: look for `ankar.exe` and the copied
  ConPTY there, not under `target\`.
