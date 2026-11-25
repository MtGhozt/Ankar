# Architecture

One process, one window, egui immediate mode. Every frame `AnkarApp::ui` (src/app.rs) rebuilds
the whole UI from state; nothing is retained between frames except that state.

## Threads

| Thread | Owner | Job |
|---|---|---|
| UI | eframe | `AnkarApp::ui`: input, layout, painting; must never block |
| `ankar-pty-<id>` loop | alacritty `EventLoop` | reads ConPTY through `OscTap`, feeds `Term` |
| `ankar-pty-<id>` events | `TerminalSession::spawn` | answers queries (`PtyWrite`, colours, size), flags bell/exit/output |
| file load | `FileViewer::open_path` | read capped file, syntect highlight, send `LoadResult` |
| `ankar-git` | `ShellFooter::refresh` | `.git/HEAD` + `git status --porcelain`, at most every 5 s |
| `ankar-folder-pick` | `FolderPick::open` | `SHBrowseForFolderW`, result over a channel |
| screenshot callback | egui | colour check read-back, logs and sets a flag |

Shared state between threads is `Arc<Mutex<..>>` / atomics owned by `TerminalSession` (cwd,
agent info, last output time, exit code, clipboard, window size).

## Terminal data flow

```
ConPTY bytes -> OscTap::reader (osc.rs scanner: OSC 7501 agent state, OSC 7 cwd,
             -> alacritty Term   OSC 12 log, CSI >q XTVERSION reply, query debug log)
Term events  -> event thread -> replies written back to the PTY (logged at debug)
UI frame     -> TerminalView::show -> session.resize -> session.sync (snapshot) -> paint_grid
```

- `sync` copies only the visible, non-default cells into `Snapshot`; hidden tabs drop theirs.
- Cell metrics come from the measured Hack reference scaled by the terminal font size
  (`ui_theme::cell_w/cell_h`, `term_row_top` snaps rows to device pixels).
- Box drawing, blocks and braille are geometry, not glyphs (view.rs), snapped to whole pixels.

## Resize

1. `TerminalView::show` asks `session.resize(w, h, metrics)` every frame.
2. `ResizeDebouncer` (80 ms) waits for the size to settle; `apply_now` skips the wait after a
   drag release, a panel toggle, the footer/viewer appearing, or a zoom.
3. `apply_size` is the only place that changes the size: grid first, then the PTY, so they
   never disagree.
4. If a frame was on screen, `sync` keeps returning it until the app has redrawn (output quiet
   for 30 ms) or 200 ms passed (`resize::hold_frame`), so TUIs never flash blank.

## Session status

- `procs::probe` (once a second, one Toolhelp32 snapshot): every tab learns `busy_child`
  (shell has a child other than conhost/OpenConsole); the active plain tab learns which agent
  runs in it (`detected`), which renames it.
- `StatusTracker::tick` per frame: an OSC 7501 report wins; otherwise `guess_busy` = child
  running and output within `waiting_timeout`. Idle shells are Waiting. Only agent-reported
  work that ends after 30 s may toast.

## Layout and panels (top to bottom, left to right)

- Left: sidebar column (`Panel::left`, width 180..420, persisted): header with the toggle, the
  Sessions/Files switch row, then the list or the file tree.
- Top: tab bar (32 px, same height as the sidebar header so both dividers line up), caption
  buttons, stats icon.
- Bottom: shell footer (26 px) only when the active tab is a plain shell.
- Centre: the tab's split tree (`render_layout`), and, when open for this tab, the file viewer
  pane on the right (`files::pane_split`, draggable ratio, maximizable).
- Overlays: palette, stats panel, zoom toast, colour check swatches.

## Persistence and files on disk

| File | Writer | Notes |
|---|---|---|
| `%APPDATA%\ankar\config.toml` | `Config::save` | not overwritten if it failed to parse |
| `%APPDATA%\ankar\sessions.json` | `persist::save_workspace` | autosave every 30 s when dirty, on exit |
| `%APPDATA%\ankar\ankar-log.txt` | `logging::init` | truncated at start; RUST_LOG else `log_level` |

## Groups

`persist::project_key_for(cwd)`: git root above the cwd (home never counts), else the cwd.
`sidebar::project_group_key`: last folder of that path, a drive root is `C:`. OSC 7 updates the
cwd of every tab each frame, and the group follows.

## Colours

- Terminal: `TermColors` (theme.rs), background `#000`, foreground `#d4d4d4`, cursor `#7699FE`.
  OSC 4/10/11/12 queries are answered from it with 16-bit channels and the query's terminator.
- Inbox Windows ConPTY swallows OSC 10/11 and DA queries; a newer `conpty.dll` +
  `x64\OpenConsole.exe` beside `ankar.exe` passes them (README, `logging::conpty_dll`).

## Memory

See `docs/ram.md`. Main costs: egui/glow base (~78 MB), scrollback grids (default 2000
lines), syntect (only while the tree or the viewer is shown).
