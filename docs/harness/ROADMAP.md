# Roadmap

Open work, most important first. Remove items when done (and add them to FEATURES.md).

## Next

- Pin tabs in the tab bar (sessions can be pinned in the sidebar only).

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
  filters the whole page; layout vs `docs/design/midterm-spec/ref/midu-stats.jpg` at 1024 px width;
  RAM open/closed per `docs/ram.md` § Stats page (08).

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
