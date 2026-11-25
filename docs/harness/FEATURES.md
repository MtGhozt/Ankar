# Features

What exists today, by theme. Update when behaviour changes; keep it short.

## Terminal

- ConPTY shells via alacritty_terminal 0.26: PowerShell, cmd, WSL profiles.
- TERM=xterm-256color, COLORTERM=truecolor, COLORFGBG=15;0, TERM_PROGRAM=Ankar + version.
- Replies: OSC 4/10/11/12 colour queries (16-bit, same terminator), DA1/DA2, XTVERSION,
  text-area size; every query and reply is logged at debug.
- Cursor shape/colour from DECSCUSR, OSC 12/112, DECTCEM.
- Box drawing, blocks and braille painted as pixel-snapped geometry; bold never dims.
- SGR and X10 mouse reporting: clicks, drag, motion, wheel, modifiers.
- OSC 52 copy (paste from apps is refused); Ctrl+Shift+C/V copy/paste.
- Palette: dark, saturated ANSI colours in the spirit of Warp; foreground `#d4d4d4`.
- Zoom: Ctrl+= / Ctrl+- / Ctrl+0, 8..24 px, "12 px" toast for a second; size persisted.
- Resize: 80 ms debounce, grid and PTY together, last frame held until the app redraws.

## Tabs, panes and sessions

- Tabs with split panes (Ctrl+Shift+D / E), focus with Alt+arrows, close with Ctrl+Shift+W.
- New session (button, Ctrl+Shift+T): default shell in the active tab's cwd, else `C:\`.
- Right-click New session: "New session in folder…" opens a native folder picker.
- Plain shells take the name/icon of the agent running inside them (process snapshot).
- Status: agents reporting OSC 7501 drive it directly; otherwise Working only while the shell
  runs a child that is printing, Waiting when idle or after a bell. Toasts only for long
  agent work that ends while unfocused (off by default).
- Restore from `%APPDATA%\ankar\sessions.json`; autosave every 30 s when something changed.
- Pins and archives; launchers row and palette entries per agent CLI.

## Sidebar

- Own full-height column: header with only the toggle, then the Sessions/Files switch row.
- Width 180..420 px, drag the edge, persisted in config.toml.
- Sessions: groups named after the project folder (git root else cwd; `C:\` shows `C:`),
  recomputed when any tab's cwd changes (OSC 7). Pinned group first.
- Rows: two lines centred, time right-aligned at the edge, pin/archive on hover left of it.
- New session button, search (Ctrl+Shift+L) and All/Waiting/Working pills keep their natural
  width; only the rows grow with the sidebar.
- Files: browse-only tree from the shell cwd, gitignored entries dimmed; "New tab here" on
  folders; the Files X hides the whole sidebar.

## File viewer

- Clicking a file opens a read-only pane right of the terminal (50/50, draggable divider),
  reusing the open pane. Tied to the tab it was opened in.
- Header: path, ⋮ menu (Maximize/Restore pane, Copy file path, Reveal in Explorer), X.
- Esc closes it when the last click was in the viewer.
- syntect highlighting and file reads off the UI thread; line numbers; 256 KB / 5000 lines cap.

## Shell footer

- Under plain shell tabs: cwd chip (click picks a folder and types `cd "<path>"`), git branch
  chip, changed-file count from `git status --porcelain` (refreshed at most every 5 s).

## Chrome

- Chromeless window with our own caption buttons; tab bar 32 px aligned with the sidebar
  header so the two bottom dividers form one line.
- Command palette (Ctrl+Shift+K): tabs, profiles, launchers, actions, archived sessions,
  "Open log folder", "Debug: check colors (log)".
- Stats panel (Ctrl+Shift+I): local usage from read-only metadata.

## Diagnostics

- `%APPDATA%\ankar\ankar-log.txt`, truncated each start; level from RUST_LOG, else
  `log_level` in config.toml, default info. Startup logs version, build mark, exe path,
  config path, ConPTY in use, font sizes.
- Every Ctrl+Shift key event eframe delivers is logged at info (other Ctrl keys at debug).
- "Debug: check colors" paints #0a0a0a, #1e1e1e, #7699FE and logs the framebuffer pixels.
