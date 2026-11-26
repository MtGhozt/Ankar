# Roadmap

Open work, most important first. Remove items when done (and add them to FEATURES.md).

## Next

- Pin tabs in the tab bar (sessions can be pinned in the sidebar only).

## Later (after stabilization)

- Home tab (fastfetch-style): a permanent, pinned main tab that cannot be closed (only when the
  app closes). Show a large ASCII logo on the left, colored system key/value info on the right
  (OS, host, kernel, uptime, shell, display, CPU, GPU, memory, disk, battery, locale), and a color
  palette strip. Read locally; no network. Reference: user's fastfetch screenshot.
- Full optimization pass: keep Ankar's own private memory as low as possible with many tabs and
  processes open (currently ~112 MB with 2 tabs, plus ~2–3 MB per console host). Profile
  allocations, glyph/texture caches, scrollback size, idle repaint, and per-session buffers.
- Release to main when stable and not crashing: merge `estabilizacion` into `main`, then publish a
  tagged GitHub Release (for example, `v0.1.0`) with a downloadable Windows installer or zip asset,
  like Capri-Desktop. The installer must create a Start Menu entry so Ankar appears in Windows
  search and can be pinned to the taskbar; include the app icon and version info, and ship
  `conpty.dll` plus `x64\OpenConsole.exe` next to `ankar.exe`. Consider an update check only later
  and only with the user's approval; Ankar has no network calls by default.
- Not before the user finishes current university projects/credits.

## Ideas (not scheduled)

- Warp-style command blocks (the input history).

## Verify by eye (written, never seen running)

- Copilot pills and Junie dark with the vendored ConPTY (only the startup log was checked).
- Sessions: Idle (no dot) -> Working -> Waiting in the real sidebar; the folder button.
- Footer chips (branch icon, +N/-N colours) and the cd picker: arrows, Enter, filter, Esc.
- Changes pane: cards, diff colours, file column, Commit/Push/Create PR, revert, Discard all.
- Notifications inbox panel; "± +N -N" and inbox icons in the tab bar.
- Window pops: when one happens, read the `window t+…` lines in the log.
- Sidebar at 180, 309 and 420 px; viewer pane, its ⋮ menu, Esc; zoom toast.
- "Debug: check colors" log shows exact matches.
- Stats (prompt 08): opens in <100 ms with skeleton; five quick range changes end on the last;
  scroll to bottom — no misdrawn charts; Esc / `esc` chip close; keys while the pane is focused
  stay out of the terminal and return after close; no red egui ID-clash text in debug; agent click
  filters the whole page; RAM open/closed per `docs/ram.md` § Stats page (08); layout at full
  width, ~740 px, and ~450 px (KPI, charts, tables keep width; header and range selector do not
  overlap; min pane ~420 px; compare `docs/design/midterm-spec/ref/midu-stats.jpg` where useful).
- Stats AGENTS: one row per tool (OpenCode modes rolled up; no `powershell.exe -nol...` rows, no
  tab titles); tooltip draws above the rows.
- Stats AGENT TIME: not `0.0 h` with every row `1m`; sub-minute time shows seconds; Junie tokens
  from `%USERPROFILE%\.junie\sessions\*\summary.json` when that file exists.
- Stats MODELS legend: not "Other 100%" for known families (Anthropic, OpenAI, Google, xAI,
  DeepSeek, Moonshot, Qwen, MiniMax, Mistral, Meta, Z.ai, OpenCode Zen free for `*-free`).
- Terminal cell row: glyphs centered in the cell (OpenCode "+ New session" pill has equal space
  above and below); Hack and Cascadia Mono compute to 14 px at 12 px and 15 px at 13 px (line gap
  0), shorter than the 16–17 px Windows Terminal figure; win metrics would be 16–17 and are not
  used; log once: `cell metrics:` in ankar-log.txt.
- ConPTY: closing a tab or quitting does not leave OpenConsole.exe / "Console Window and PTY Host"
  behind; the in-app "Ankar: N MB private · M console hosts" line was NOT built (Private Bytes
  needs windows-sys feature `Win32_System_ProcessStatus`, not enabled).

## Leftovers

- Window pops were not reproduced. Guard in place: minimized or tiny frames never resize the
  terminals; a DPI change applies the new grid at once. If pops remain, the log says when.
- No file watcher crate: git facts refresh by polling every 5 s and right after an action.
- Discard all keeps untracked files (`git reset --hard`); revert deletes one untracked file.
- Inbox lists agent-done and non-zero exits only; agents have no "needs input" report.
- Sidebar has no Idle filter pill: the filter bar keeps a fixed natural width.
- Status: a TUI that keeps printing while idle stays Working until `waiting_timeout_secs`.
- RAM scenarios B–E in `docs/ram.md` are still unmeasured (changes pane included).
- Viewer pane as a real layout leaf, so it splits the focused pane, not the whole tab area;
  persist its ratio.
- Esc focus: clicking the viewer does not take keyboard focus from the terminal.
- Spec says palette on Ctrl+K; the binding is Ctrl+Shift+K (Ctrl+K belongs to shells).
- Group keys are case-sensitive now; collapsed groups saved in lowercase need re-collapsing.
- Stats AGENTS subline shows "N agents" — no real concurrency metric yet.
- Ankar / cursor-agent have no token metadata; filtering by them leaves token charts at zero.
