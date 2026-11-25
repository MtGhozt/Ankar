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
Symptom: the prompt 14 report said Junie was dark, Copilot fixed and Ctrl+Shift+T working;
in the real app none of it was true.
Root cause: changes were "verified" by tests and reasoning only; nobody looked at the app.
Fix: reports list what was not verified by eye (prompt 15).
Guard: AGENTS.md rule "never claim something visual works unless you saw it".

## 2026-11-25 — Approved UI moved
Symptom: 09b9bb1 moved the Sessions/Files switch into the top strip, which Matias had approved
where it was.
Root cause: a layout task was read as permission to rearrange neighbouring UI.
Fix: switch back under the toggle row (39cad81).
Guard: `sidebar::tests::files_close_hides_the_whole_sidebar_and_the_switch_sits_under_the_header`;
rule: only change the UI a task lists.

## 2026-11-25 — Inbox ConPTY swallows terminal queries
Symptom: Junie stayed on its light theme; Copilot could not read the palette.
Root cause: Windows' built-in ConPTY strips OSC 10/11 and DA from the app's output, so the
query never reaches Ankar. A newer conpty.dll + OpenConsole.exe passes them.
Fix: logged at startup which ConPTY is used; README explains shipping the dll (bb650c0).
Guard: `terminal::osc::tests::conpty_passes_the_osc_11_background_query_through` (ignored;
fails on inbox ConPTY by design).

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
