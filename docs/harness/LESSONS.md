# Lessons

One entry per mistake that cost time. Newest last. Format:

```
## YYYY-MM-DD — short title
Symptom: what the user saw.
Root cause: why.
Fix: what changed (commit).
Guard: the test or rule that keeps it fixed.
```

## 2026-11-24 — Staircase output after a resize
Symptom: TUIs drew diagonal "staircase" text after the window or sidebar changed size.
Root cause: the grid and the PTY were resized at different times, so the app and the grid
disagreed about the width.
Fix: one `apply_size` changes grid then PTY together, behind an 80 ms debounce (30c1df5, d105495).
Guard: `terminal::resize::tests::*`; rule: never resize the PTY anywhere else.

## 2026-11-24 — Bold text rendered dim
Symptom: every bold cell looked faded.
Root cause: `Flags::DIM_BOLD` is `DIM | BOLD`; `intersects(DIM_BOLD)` is true for bold alone.
Fix: test `DIM` on its own (0416d53).
Guard: `terminal::view::tests::truecolor_is_rendered_as_sent_and_dim_only_on_request`.

## 2026-11-24 — Dashed box drawing and seams between rows
Symptom: box-drawing lines and block glyphs showed gaps and dashes.
Root cause: glyphs from the font, at fractional positions, do not tile.
Fix: draw them as rects snapped to device pixels (7108dc5, 4cad4e7, 755d60a).
Guard: `terminal::view::tests::block_rects_share_whole_pixel_edges` and the heavy-line tests.

## 2026-11-24 — No hover in OpenCode
Symptom: hover highlights in OpenCode never appeared.
Root cause: mouse motion was not reported to the PTY.
Fix: SGR motion, drag, wheel and modifiers, deduplicated per cell (4e57942).
Guard: `terminal::backend::tests::sgr_reports_motion_release_wheel_and_modifiers`.

## 2026-11-25 — PowerShell wrote UTF-16 files
Symptom: files written from the shell broke the build or showed garbage accents.
Root cause: Windows PowerShell `Set-Content`, `Out-File` and `>` write UTF-16 or add a BOM.
Fix: write files with the editor tools only.
Guard: AGENTS.md hard rule (UTF-8 without BOM, LF).

## 2026-11-25 — Warp is AGPL
Symptom: the temptation to paste Warp code for a matching look.
Root cause: Warp's source is AGPL; Ankar is MIT.
Fix: take the idea (layout, behaviour), write our own code.
Guard: AGENTS.md hard rule.

## 2026-11-25 — Heavy work on the UI thread
Symptom: stutter when opening files or big folders.
Root cause: reading and highlighting ran inside the frame.
Fix: file load + syntect on a thread, capped reads, a dir cache; git and dialogs on threads.
Guard: AGENTS.md rule; review any `fs::`/`Command` call reachable from `ui()`.

## 2026-11-25 — Visual fixes claimed but never seen
Symptom: the prompt 14 report said Junie was dark and Copilot fixed; in the real app neither
was true. (Ctrl+Shift+T was never broken: Matias was pressing Ctrl+Alt+T, see the r15 log.)
Root cause: changes were "verified" by tests and reasoning only; nobody looked at the app.
Fix: reports list what was not verified by eye (prompt 15).
Guard: AGENTS.md rule "never claim something visual works unless you saw it".

## 2026-11-25 — Approved UI moved
Symptom: 09b9bb1 moved the Sessions/Files switch into the top strip; prompt 14 then made the
sidebar a full-height column and pushed the tab bar right of it. Matias had approved the
full-width tab bar with the toggle at the window's top-left.
Root cause: a layout task was read as permission to rearrange neighbouring UI.
Fix: switch back (39cad81); full-width tab bar, toggle top-left, sidebar below (b24280b).
Guard: `app::tests::toggle_sits_top_left_centred_above_the_divider_and_tabs_follow_it`,
`sidebar::tests::files_close_hides_the_whole_sidebar_and_the_switch_sits_at_the_top`;
rule: only change the UI a task lists.

## 2026-11-25 — Inbox ConPTY swallows terminal queries
Symptom: Junie stayed on its light theme; Copilot could not read the palette (no pills).
Root cause: Windows' built-in ConPTY strips OSC 10/11/4 and DA from the app's output, so the
query never reaches Ankar. Matias confirmed with Warp's newer conpty.dll on PATH (r15).
Fix: ship Microsoft.Windows.Console.ConPTY 1.25.261109001 in `vendor/conpty`; build.rs puts
`conpty.dll` and `x64\OpenConsole.exe` beside `ankar.exe` (5ad8b71).
Guard: `terminal::osc::tests::conpty_passes_the_osc_11_background_query_through` (ignored;
green with the vendored dll); the startup log names the ConPTY in use.

## 2026-11-25 — build.rs copied into the wrong folder
Symptom: the ConPTY copy step "worked" but nothing appeared beside `ankar.exe`.
Root cause: the sandbox redirects `CARGO_TARGET_DIR`, and Cargo 1.100 nests build scripts as
`build\ankar\<hash>\out`, so a fixed `OUT_DIR` ancestor count pointed at `release\build`.
Fix: walk up to the ancestor named `build` and take its parent (5ad8b71).
Guard: look for the files under `$env:CARGO_TARGET_DIR`, not `target\`.

## 2026-11-25 — Logging must write a file
Symptom: "no log file" after asking for logs; release builds have no console.
Root cause: env_logger wrote to stderr only, and `windows_subsystem = "windows"` drops it.
Fix: always write `%APPDATA%\ankar\ankar-log.txt` (c133845).
Guard: `logging::tests::init_truncates_the_file_and_writes_records_to_it`.

## 2026-11-25 — The gate was green only in release
Symptom: `cargo test` panicked "Dropped TexturesDelta" in three app tests.
Root cause: headless egui tests dropped `FullOutput` with pending texture deltas; epaint
asserts on that in debug only.
Fix: tests clear `textures_delta` before dropping the output (40aa4a7).
Guard: TESTING.md headless pattern; the gate runs `cargo test` in debug.

## 2026-11-25 — One long chat ate the token budget
Symptom: prompt 16 burned ~6% of Matias's monthly Cursor usage in a single conversation.
Root cause: 11 steps in one chat (every tool call re-sends the growing context), full
cargo/clippy/test output pasted in, re-reading large files, and extra tests or "double-check"
passes nobody asked for.
Fix: `AGENTS.md` § Token budget; `ankar-step` subagent; lesson recorded here (prompt 17).
Guard: coordinator + subagents; quiet gate; narrow reads; report ≤10 lines per step.

## 2026-11-25 — an "inherit" subagent in 19 ran on Grok 4.7 High Fast, so unpinned subagents are banned.
