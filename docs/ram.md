# Ankar RAM measurements

Metric: Working Set (WS) and Private Bytes via `Get-AnkarMem` (ankar.exe alone vs full process tree).

Scenarios:

- **A**: 1 PowerShell tab, ~10–12 s idle
- **B**: 3 PowerShell tabs idle
- **C**: 3 tabs each filled with `1..20000 | ForEach-Object { ("linea $_ ") * 8 }`, then idle (~10 s after fills finish)

## Before (pre-change release)

| Scenario | ankar WS | ankar Private | tree WS | tree Private |
|---|---:|---:|---:|---:|
| A | 84.8 | 85.8 | 180.4 | 145.4 |
| B | — | — | — | — |
| C | — | — | — | — |

Notes:

- Before B/C were not re-measured with `Get-AnkarMem` (Task Manager previously reported ~87 MB / ~148 MB for 1 / 3 tabs; that figure includes the process tree).
- Before A tree included **two** `conhost.exe` (app console + ConPTY).

## After (P1–P6)

| Scenario | ankar WS | ankar Private | tree WS | tree Private |
|---|---:|---:|---:|---:|
| A | 78.2 | 78.4 | 162.9 | 136.3 |
| B | 87.9 | 97.7 | 342.2 | 271.4 |
| C | 111.0 | 104.4 | 111.0 | 104.4 |

Notes:

- After A tree has **one** `conhost` (no extra console; `windows_subsystem = "windows"`).
- Scenario C tree shrinks because the fill commands exit; remaining Private is mostly ankar (3× ~3000-line grids + egui/glow base).
- Goal was ankar Private **&lt; 100 MB** on C: measured **104.4 MB** (≈4 MB over). Dominant cost is egui/glow base (~78 MB idle) plus three full scrollback grids.

## Experiments (P7)

| Change | Effect on C Private |
|---|---|
| Disable eframe `accesskit` | ~104.1 MB (no meaningful win); **reverted** to keep accessibility |
| `mimalloc` | not tried (needs explicit OK; new C dep) |

## What moved the needle

- Viewport snapshot instead of per-frame full `Grid` clone (P1)
- Scrollback default 3000 (P2)
- Coalesced / visibility-gated repaints (P3)
- Row `LayoutJob` text (P4) — CPU / transient
- Lazy syntect + capped file read + dir cache (P5)
- No release console window (P6)
