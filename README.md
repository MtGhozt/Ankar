# Ankar

A clean desktop terminal for Windows. Fast, local-only, no accounts, no telemetry, no AI.

Built for running and organizing terminal agents (OpenCode, cursor-agent, gemini, …) per project.

## Features

- Tabs and split panes (horizontal / vertical)
- Shell profiles: PowerShell, cmd, WSL (distros via `wsl -l -q`)
- Session sidebar grouped by project folder, with pins and archives
- Session restore from `%APPDATA%\ankar\sessions.json`
- Status detection: active / waiting / finished + Windows toasts when unfocused
- Midterm-style UI: pure black terminal, gray chrome, project-grouped session sidebar
- Configurable launchers (sidebar icon row + palette; Lucide icon + color per launcher)
- Command palette (`Ctrl+Shift+K`)
- Read-only project file viewer with syntect highlighting
- Zero network calls from the app itself
- Fonts: Inter (UI) + Lucide subset (icons) — see `assets/fonts/LICENSES.md`

## Agents

Coding agents working on this repo start at [`AGENTS.md`](AGENTS.md).

## Install / build

Requirements:

- Rust stable (MSVC toolchain)
- Visual Studio Build Tools with C++ workload

```powershell
git clone https://github.com/MtGhozt/ankar.git
cd ankar
cargo build --release
.\target\release\ankar.exe
```

### Recommended: a current ConPTY next to `ankar.exe`

The ConPTY built into Windows swallows terminal queries (OSC 10/11 background colour, DA), so
TUIs such as Junie fall back to a light theme and Copilot cannot read the palette. Take
`conpty.dll` and `OpenConsole.exe` (MIT) from the `Microsoft.Windows.Console.ConPTY` NuGet
package and lay them out like this (the layout Warp ships):

```
ankar.exe
conpty.dll
x64\OpenConsole.exe
```

Ankar logs which ConPTY it uses at startup
(`ConPTY: ...` in `%APPDATA%\ankar\ankar-log.txt`). Check with
`cargo test conpty_passes -- --ignored` (passes only with the newer ConPTY).

## Migrating from nacar

On first launch, Ankar copies `config.toml` and `sessions.json` from `%APPDATA%\nacar` to `%APPDATA%\ankar` if those files do not already exist in the new folder. The old `%APPDATA%\nacar` folder is left in place so you can delete it yourself after confirming the migration.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+T` | New tab (default shell) |
| `Ctrl+Shift+W` | Close focused pane (or tab) |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Alt+←` `↑` `↓` `→` | Focus previous / next pane |
| `Ctrl+Shift+D` | Split vertical |
| `Ctrl+Shift+E` | Split horizontal |
| `Ctrl+Shift+K` | Command palette |
| `Ctrl+Shift+B` | Toggle sessions sidebar |
| `Ctrl+Shift+F` | Toggle file viewer |
| `Ctrl+Shift+I` | Agent usage stats (local-only) |
| `Ctrl+Shift+L` | Focus session search in sidebar |
| `Ctrl+Shift+P` | Pin / unpin active session |
| `Ctrl+Shift+N` | Profile picker |
| `Ctrl+1`…`9` | Switch tab |
| `Ctrl+Shift+S` | Save session layout now |
| `Ctrl+Shift+C` / `V` | Copy / paste in terminal |
| `Ctrl+C` | Copy selection, or interrupt (0x03) if none |
| `Ctrl+X` | Send 0x18 (cancel) to the shell |

## Configuration

On first run Ankar creates:

`%APPDATA%\ankar\config.toml`

Example:

```toml
# Default shell profile id: powershell | cmd | wsl | wsl:<distro>
default_shell = "powershell"

# Seconds without PTY output before a live session is "waiting"
waiting_timeout_secs = 30

font_size = 13.0
accent = "#4D9FFB"

# Scrollback lines per terminal (default 2000; hard cap 50000)
scrollback_lines = 2000

sidebar_width = 280.0
dim_inactive_panes = true

pinned_projects = []
pinned_sessions = []
collapsed_groups = []

[[launchers]]
name = "opencode"
command = "opencode"
args = []
icon = "sparkles"
color = "#D97757"

[[launchers]]
name = "cursor-agent"
command = "cursor-agent"
args = []
icon = "bot"
color = "#8A8A8A"
```

Session layout is stored in `%APPDATA%\ankar\sessions.json` (tabs, panes, folders, commands — not scrollback).

Bundled fonts under `assets/fonts/` — Inter (OFL-1.1) and Lucide icon subset (ISC). Details in [`assets/fonts/LICENSES.md`](assets/fonts/LICENSES.md).

## Colored paths in the prompt

Ankar does **not** rewrite your prompt. Use a prompt engine inside the shell:

### oh-my-posh (PowerShell)

```powershell
winget install JanDeDobbeleer.OhMyPosh
oh-my-posh init pwsh --config "$env:POSH_THEMES_PATH\tanuki.omp.json" | Invoke-Expression
```

Add that line to your `$PROFILE`.

### starship

```powershell
winget install Starship.Starship
# In $PROFILE:
Invoke-Expression (&starship init powershell)
```

Pick a theme that colors the cwd segment; alacritty_terminal already renders the ANSI colors Ankar receives.

## Shell integration (opcional)

Para que el visor de archivos siga los `cd` en PowerShell, agregá esto al final de `$PROFILE` (editándolo con el editor, p. ej. `cursor $PROFILE`, nunca con `Set-Content`), y **después** del init de oh-my-posh o starship si se usa:

```powershell
$__ankarPrompt = $function:prompt
function prompt {
  $loc = $executionContext.SessionState.Path.CurrentLocation
  if ($loc.Provider.Name -eq 'FileSystem') { $Host.UI.RawUI.WindowTitle = $loc.ProviderPath }
  & $__ankarPrompt
}
```

(ConPTY traduce el cambio de título a OSC 0/2, y alacritty lo entrega como `Event::Title`.) Ankar no modifica el prompt por su cuenta.

## Privacy

- No analytics, no accounts, no cloud sync
- No HTTP client in the dependency set used by the app
- Notifications are local Windows toasts only

## License

MIT
