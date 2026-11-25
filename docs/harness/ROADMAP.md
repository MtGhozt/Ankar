# Roadmap

Open work, most important first. Remove items when done (and add them to FEATURES.md).

## Next

- Stats panel UI (prompt 08): not started on this branch.
- Ship a current ConPTY: `conpty.dll` + `x64\OpenConsole.exe` beside `ankar.exe` (build step
  or README only). Waiting for Matias to approve the download.
- Pin tabs in the tab bar (sessions can be pinned in the sidebar only).
- Tab bar: `fixed_left` in `draw_tab_bar` leaves a 13 px gap left of the first tab.

## Verify by eye (written, never seen running)

- Sidebar rows and controls at 180, 309 and 420 px; no flicker while dragging the edge.
- Viewer pane, its ⋮ menu, Reveal in Explorer, Esc; footer chips and the cd picker.
- Zoom toast and that no stale pixels remain after zooming.
- Junie dark and Copilot tabs after shipping the newer ConPTY.
- "Debug: check colors" log shows exact matches.

## Leftovers

- Copilot bracketed tabs: it needs OSC 10, 11 and all 16 OSC 4 palette answers within
  100 ms each; with the inbox ConPTY they never arrive. Recheck after the ConPTY ships.
- RAM scenarios B–E in `docs/ram.md` are still unmeasured.
- Git-branch icon for the footer (the bundled Lucide subset has none; needs a new subset).
- Viewer pane as a real layout leaf, so it splits the focused pane, not the whole tab area;
  persist its ratio.
- Esc focus: clicking the viewer does not take keyboard focus from the terminal.
- Spec says palette on Ctrl+K; the binding is Ctrl+Shift+K (Ctrl+K belongs to shells).
- Group keys are case-sensitive now; collapsed groups saved in lowercase need re-collapsing.
