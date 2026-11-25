# Features

What exists today, by theme. Update when behaviour changes; keep it short.

## Terminal

- ConPTY shells via alacritty_terminal 0.26: PowerShell, cmd, WSL profiles. A current
  ConPTY ships in `vendor/conpty` (build.rs copies it beside `ankar.exe`) so OSC 4/10/11 work.
- Font 12 px by default (an old 13 is migrated once, `config_version`); rows are 1.2x the
  font size with glyphs centred (14.4 px at 12).
- TERM=xterm-256color, COLORTERM=truecolor, COLORFGBG=15;0, TERM_PROGRAM=Ankar + version.
- Replies: OSC 4/10/11/12 colour queries (16-bit, same terminator), DA1/DA2, XTVERSION,
  text-area size; every query and reply is logged at debug.
- Cursor shape/colour from DECSCUSR, OSC 12/112, DECTCEM.
- Box drawing, blocks and braille painted as pixel-snapped geometry; bold never dims.
- SGR and X10 mouse reporting: clicks, drag, motion, wheel, modifiers.
- OSC 52 copy (paste from apps is refused); Ctrl+Shift+C/V copy/paste.
- Palette: dark, saturated ANSI colours in the spirit of Warp; foreground `#d4d4d4`.
- Zoom: Ctrl+= / Ctrl+- / Ctrl+0, 8..24 px, "12 px" toast for a second; size persisted.
- Resize: 80 ms debounce, grid and PTY together, last frame held until the app redraws;
  minimized or tiny transient frames never resize; a DPI change applies at once.

## Tabs, panes and sessions

- Tabs with split panes (Ctrl+Shift+D / E), focus with Alt+arrows, close with Ctrl+Shift+W.
- New session (button, Ctrl+Shift+T): default shell in the active tab's cwd, else `C:\`.
- Folder button right of New session (and its right-click menu): new session in a picked folder.
- Plain shells take the name/icon of the agent running inside them (process snapshot).
- Status: Idle (no dot) until the first work; Working while an agent reports it (OSC 7501)
  or, after Enter, while a child prints; Waiting once that work stops; Finished on exit.
  Toasts only for long agent work that ends while unfocused (off by default).
- Notifications inbox (tray icon, tab bar): agent done and non-zero exits, newest first, time
  and tab name, "All tabs"/"This tab" chip, click to go to the tab; 100 max, memory only.
- Restore from `%APPDATA%\ankar\sessions.json`; autosave every 30 s when something changed.
- Pins and archives; launchers row and palette entries per agent CLI.

## Sidebar

- Below the full-width tab bar; the Sessions/Files switch row is its first row.
- Width 180..420 px, drag the edge, persisted in config.toml.
- Sessions: groups named after the project folder (git root else cwd; `C:\` shows `C:`),
  recomputed when any tab's cwd changes (OSC 7). Pinned group first.
- Rows: two lines centred, time right-aligned at the edge, pin/archive on hover left of it.
- New session button, search (Ctrl+Shift+L) and All/Waiting/Working pills keep their natural
  width; only the rows grow with the sidebar.
- Files: browse-only tree from the shell cwd, gitignored entries dimmed; "New tab here" on
  folders; the Files X hides the whole sidebar.

## File viewer

- Clicking a file (tree, or a changes card) opens a read-only pane right of the terminal
  (50/50, draggable divider), reusing the open pane. Tied to the tab it was opened in.
- Header: path, ⋮ menu (Maximize/Restore pane, Copy file path, Reveal in Explorer), X.
- Esc closes it when the last click was in the viewer.
- syntect highlighting and file reads off the UI thread; line numbers; 256 KB / 5000 lines cap.

## Shell footer

- Under plain shell tabs, Warp-style chips (#1a1a1a, 1 px #333, 11 px): folder + cwd, drawn
  git-branch icon + branch, file icon + changed files with +N green / -N red.
- The cwd chip opens an in-app picker above it: "Search directories...", ".. (Parent
  Directory)", folders then greyed files; arrows or click, Enter cds, typing filters, Esc.
- The files chip opens the changes pane.

## Changes pane

- "± +N -N" in the tab bar (or the footer chip) opens it in the right-hand slot it shares with
  the viewer. Header: repo, branch, counts, Commit menu (Commit / Push / Create PR), ⋮
  (Discard all, red confirm), hide file navigation, maximize, X.
- "Uncommitted changes": one collapsible card per file with path, copy path, +N -N, revert
  (asks first), open in viewer; unified diff with line numbers, green/red lines; binaries say
  "Binary file - no diff available". File column on the right scrolls to a card.
- Commit is `git commit -a -m`; Push asks first; Create PR opens
  `github.com/<owner>/<repo>/compare/<branch>` in the browser. All git runs on threads; facts
  refresh every 5 s and right after an action.

## Chrome

- Chromeless window with our own caption buttons; full-width tab bar (37 px) with the
  sidebar toggle at the top-left; 1 px #2E2E2E outline round the window (not when maximized).
- Right of the tabs: "± +N -N" (in a git repo), inbox, stats, caption buttons.
- Command palette (Ctrl+Shift+K): tabs, profiles, launchers, actions, archived sessions,
  "Open log folder", "Debug: check colors (log)".
- Stats panel (Ctrl+Shift+I): local usage from read-only metadata.

## Diagnostics

- `%APPDATA%\ankar\ankar-log.txt`, truncated each start; level from RUST_LOG, else
  `log_level` in config.toml, default info. Startup logs version, build mark, exe path,
  config path, ConPTY in use, font sizes.
- Every key pressed with Ctrl is logged at trace only (`log_level = "ankar=trace"`).
- Window focus, occlusion, minimize, maximize, DPI and size changes are logged at info as
  `window t+<ms>: …`, with "(layout held)" for frames that may not resize the terminals.
- "Debug: check colors" paints #0a0a0a, #1e1e1e, #7699FE and logs the framebuffer pixels.
