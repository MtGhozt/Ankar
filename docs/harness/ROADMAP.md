# Roadmap

Open work, most important first. Remove items when done (and add them to FEATURES.md).

## Next

- Stats panel via prompt 18.
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
