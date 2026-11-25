# Ankar: agent guide

Ankar is a Windows desktop terminal written in Rust: eframe/egui 0.37 (glow), alacritty_terminal
0.26, ConPTY. No accounts, no telemetry, no network, no in-app AI.

## Before starting

1. Read this file, then `docs/harness/LESSONS.md` (mistakes not to repeat).
2. Skim `docs/harness/ARCHITECTURE.md` for the modules you will touch.
3. Check `docs/harness/FEATURES.md` so you do not rebuild or break something that exists.
4. Read `docs/harness/TESTING.md`: the gate, headless egui tests, what cannot be tested.
5. `git status` and `git log --oneline -15`: know the branch and what is uncommitted.

## Repo map

| Path | What lives there |
|---|---|
| `src/main.rs` | entry point, logging setup, window options, build mark |
| `src/app.rs` | `AnkarApp`: frame loop, tab bar, panels, shortcuts, splits, viewer pane, zoom |
| `src/sidebar.rs` | Sessions list (groups, rows, filters, search) and the Sessions/Files switch |
| `src/files.rs` | file tree (browse only) and the read-only viewer pane |
| `src/footer.rs`, `src/gitinfo.rs` | shell footer chips; git branch, changed files, +/- counts |
| `src/dir_picker.rs` | in-app cd picker above the footer's cwd chip |
| `src/changes.rs` | git watcher, diff parsing, changes pane (commit/push/PR/revert/discard) |
| `src/inbox.rs` | notifications inbox panel |
| `src/window_watch.rs` | window focus/DPI/size log; holds terminal resizes on bogus frames |
| `src/folder_pick.rs` | native folder dialog on a worker thread |
| `src/layout.rs` | `Workspace`, `Tab`, split tree, where a new session starts |
| `src/status.rs`, `src/procs.rs` | Working/Waiting/Finished; process snapshot (agents, busy shells) |
| `src/persist.rs` | `sessions.json`, project keys (git root else cwd) |
| `src/config.rs` | `%APPDATA%\ankar\config.toml` |
| `src/logging.rs` | `%APPDATA%\ankar\ankar-log.txt`, ConPTY dll lookup |
| `src/palette.rs` | command palette (Ctrl+Shift+K) |
| `src/color_check.rs` | framebuffer read-back of known colours (palette debug command) |
| `src/terminal/backend.rs` | `TerminalSession`: PTY, event thread, query replies, snapshot |
| `src/terminal/osc.rs` | raw-byte scanner: OSC 7501/7/12, XTVERSION, query logging |
| `src/terminal/view.rs` | grid painting, box drawing, cursor, keyboard and mouse input |
| `src/terminal/resize.rs` | resize debounce and the held frame after a resize |
| `src/terminal/theme.rs`, `src/ui_theme.rs` | terminal palette; UI colours, fonts, icons |
| `src/stats/` | local usage stats panel (read-only metadata) |
| `build.rs`, `vendor/conpty/` | Microsoft ConPTY copied beside `ankar.exe` |
| `docs/ram.md` | RAM measurements and method |
| `docs/harness/` | this harness |

## Hard rules

- The stack is fixed: eframe/egui 0.37 with glow, alacritty_terminal 0.26, ConPTY. Ask Matias
  before adding a crate.
- No network (at runtime or while working), no telemetry, no in-app AI.
- Warp (github.com/warpdotdev/warp) is AGPL: copy ideas, never code.
- RAM: `ankar.exe` stays under 100 MB Private Bytes with 3 tabs (`docs/ram.md`). Heavy state
  (syntect, file text, hidden tabs' cells) is dropped when not visible.
- Keep the UI thread light: file reads, git, highlighting and dialogs run on threads.
- Stats privacy: read-only metadata. Never open `auth.json`, `cli-config.json`, OpenCode's
  `log/` or `snapshot/`, or Cursor's `store.db`; never read prompt or message text.
- Files are UTF-8 without BOM, LF. Never write them with PowerShell `Set-Content`, `Out-File`
  or `>` (they write UTF-16 or a BOM); use the editor tools.
- Only change the UI a task lists. Approved layout stays where it is.
- Never claim something visual works unless you saw it; report it as "not verified by eye".
- Work stays on the local branch; push only when Matias asks. One commit per step.

## Token budget

1. **Orchestrate, don't do.** For any prompt with more than 2 steps, the main chat is a
   coordinator: it delegates each step to a subagent (one subagent per step, sequential, never
   parallel on the same files) and keeps only the subagent's short report. Mechanical steps
   (docs, logs, renames, small UI tweaks, config) go to the `ankar-step` subagent (Composer
   2.5, no Fast); only hard steps (new modules, threading, parsing, tricky bugs) use Grok 4.7
   Medium. The coordinator never reads source files itself beyond what it needs to brief the
   subagent.
2. **Subagent brief** = the step text + file hints + the gate. **Subagent report back** = max
   ~10 lines: what changed, files, commit hash, gate result, anything blocked. No diffs, no logs
   pasted back.
3. **Read narrow.** Find code with `rg -n` and read only the line range needed. Never read a
   whole file over ~300 lines. Don't re-read a file you just edited.
4. **Quiet commands.** Iterate with `cargo check -q`. Gate once per step, at the end, quiet:
   `cargo build --release -q`, `cargo clippy --all-targets -q -- -D warnings`, `cargo test -q`.
   On failure, look only at the first error, not the full output.
5. **No extra work.** No tests beyond what the prompt asks for (no smoke tests, no headless
   render tests unless requested). No "let me verify the binary / double-check the commit"
   passes: if `git commit` exits 0 and the gate passed, move on.
6. **Short status.** No narration between tool calls. Final report: one line per step (commit
   hash + one sentence) plus open issues.
7. **One chat per 2–3 steps** if subagents aren't available: Matias opens a new chat with "do
   steps X–Y of NN-*.md, read AGENTS.md first".

## Gate (every code commit)

```powershell
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo test
```

All three green. `cargo test` runs in debug, where egui's debug asserts fire.

## TDD loop

1. Bug: first write a test that fails for the reported reason; run it and watch it fail.
2. Make it pass with the smallest change. No drive-by refactors.
3. Run the full gate, then commit with a message that says what changed for the user.
4. Features: put the logic in a pure function and test that (layout maths, parsers, status
   decisions); keep the egui drawing thin.
5. What cannot be tested headlessly (pixels, a real ConPTY, focus, other apps) goes in the
   report as "not verified".

## After every task

- Add a `docs/harness/LESSONS.md` entry for any mistake that cost time.
- Update `FEATURES.md` for new or changed behaviour, `ROADMAP.md` for what is left,
  `ARCHITECTURE.md` when a module or data flow changes, `TESTING.md` for new test patterns.
- Keep each file within its line budget: replace stale lines instead of appending forever.
