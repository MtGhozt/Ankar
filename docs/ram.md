# Ankar RAM measurements

Metric: Working Set (WS) and Private Bytes via `Get-AnkarMem` (ankar.exe alone vs full process tree).

Windows Task Manager can show many "Console Window and PTY Host" entries (~1.6 MB each, tree
~2 GB) so ankar.exe is easy to miss; those hosts are OpenConsole.exe from ConPTY. As of aa631a9,
closing a tab or quitting kills that session's host. A stats footer (`Ankar: N MB private · M console
hosts`) was not added: Private Bytes needs windows-sys `Win32_System_ProcessStatus` (not enabled).
Host count alone is possible via ToolHelp (`src/procs.rs`).

Scenarios:

- **A**: 1 PowerShell tab, ~10–12 s idle
- **B**: 3 PowerShell tabs idle
- **C**: 3 tabs each filled past the scrollback cap (`1..N | ForEach-Object { ("linea $_ ") * 8 }` with N ≫ scrollback), then idle (~10 s after fills finish)

## Before (pre-change release)

| Scenario | ankar WS | ankar Private | tree WS | tree Private |
|---|---:|---:|---:|---:|
| A | 84.8 | 85.8 | 180.4 | 145.4 |
| B | — | — | — | — |
| C | — | — | — | — |

Notes:

- Before B/C were not re-measured with `Get-AnkarMem` (Task Manager previously reported ~87 MB / ~148 MB for 1 / 3 tabs; that figure includes the process tree).
- Before A tree included **two** `conhost.exe` (app console + ConPTY).

## After (P1–P6, scrollback default 3000)

| Scenario | ankar WS | ankar Private | tree WS | tree Private |
|---|---:|---:|---:|---:|
| A | 78.2 | 78.4 | 162.9 | 136.3 |
| B | 87.9 | 97.7 | 342.2 | 271.4 |
| C | 111.0 | 104.4 | 111.0 | 104.4 |

Notes:

- After A tree has **one** `conhost` (no extra console; `windows_subsystem = "windows"`).
- Scenario C tree shrinks because the fill commands exit; remaining Private is mostly ankar (3× ~3000-line grids + egui/glow base).
- Goal was ankar Private **&lt; 100 MB** on C: measured **104.4 MB** (≈4 MB over). Dominant cost is egui/glow base (~78 MB idle) plus three full scrollback grids.

## After (scrollback default 2000)

`scrollback_lines` in `%APPDATA%\ankar\config.toml` (default **2000**, hard cap 50_000). Measured with default 2000, three fill tabs (`1..5000` each), idle after fills exited.

| Scenario | ankar WS | ankar Private | tree WS | tree Private |
|---|---:|---:|---:|---:|
| C | 93.7 | 91.9 | 93.7 | 91.9 |

Notes:

- C Private **91.9 MB** (was 104.4 at 3000) — about **12.5 MB** less; under the **&lt; 100 MB** goal.
- Tree equals ankar alone because the `-Command` fill shells exited; grids stay in process memory.

## Experiments (P7)

| Change | Effect on C Private |
|---|---|
| Disable eframe `accesskit` | ~104.1 MB (no meaningful win); **reverted** to keep accessibility |
| `mimalloc` | not tried (needs explicit OK; new C dep) |
| Scrollback default 3000 → 2000 | C Private 104.4 → **91.9 MB** |

## What moved the needle

- Viewport snapshot instead of per-frame full `Grid` clone (P1)
- Scrollback default 3000, then **2000** (P2 / follow-up); configurable via `scrollback_lines`
- Coalesced / visibility-gated repaints (P3)
- Row `LayoutJob` text (P4) — CPU / transient
- Lazy syntect + capped file read + dir cache (P5)
- No release console window (P6)

## After stabilization (07)

Metric: `ankar.exe` alone (not the process tree), Private Bytes and Working Set in MB, read with:

```powershell
Get-Process ankar | Select-Object @{n='PrivateMB';e={[math]::Round($_.PrivateMemorySize64/1MB,1)}}, @{n='WSMB';e={[math]::Round($_.WorkingSet64/1MB,1)}}
```

`cargo build --release`, ~10 s idle before each reading. "Before" is the ≈140 MB Matias saw in Task Manager on `main` at 4f0f2ab.

| Scenario | Before | ankar Private | ankar WS |
|---|---:|---:|---:|
| A: 1 PowerShell tab | ≈140 | 104.4 | 109.8 |
| B: 3 PowerShell tabs | ≈140 | pending | pending |
| C: 3 tabs, scrollback full (`1..5000 \| % { ("linea $_ ") * 8 }`) | — | pending | pending |
| D: B + file viewer with `Cargo.lock` open | — | pending | pending |
| D: B + file viewer closed again | — | pending | pending |
| E: 1 PowerShell + 1 OpenCode tab | — | pending | pending |

Notes:

- A was read 15 s after launch; 15 s later it was 105.1 Private / 116.7 WS. Idle CPU over 10 s was 0.01 s (the 10 fps filter-bar repaint is gone).
- B–E need tabs opened and commands typed inside Ankar; they were not measured in the stabilization session.
- Goal: B < 100 MB Private (ideally C too). A alone is already ≈4.4 MB over it, so B will not meet the goal without further work. The stats panel is not the cause: its loader only runs when the panel is opened.

## Stats page (08)

Metric: same `Get-Process ankar` Private MB line as above. Gate release build; measured 2026-11-25.

| Scenario | ankar Private |
|---|---:|
| Stats closed (5 tabs: junie, copilot, opencode launchers + 2 PowerShell; Matias's restored session) | 116.0 |
| Stats open, range 30 days | not measured |
| Stats closed again | not measured |

Notes:

- Not comparable to the <100 MB goal or section 07's 3-tab method (here: 5 tabs, not 3 PowerShell).
- Open and closed-again were not measured in the stabilization session (GUI driving was blocked in the Cursor sandbox).
- Matias should run the 3-tab method: closed → open 30 days → closed again (~10 s idle before each read). Target: closed-again within 3–5 MB of the first closed reading; closed < 100 MB Private.
