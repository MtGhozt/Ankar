# Ankar

A clean desktop terminal for Windows. Fast, local-only, no accounts, no telemetry, no AI.

Built for running and organizing terminal agents (OpenCode, cursor-agent, gemini, …) per project.

## Features

- Tabs and split panes (horizontal / vertical)
- Shell profiles: PowerShell, cmd, WSL (distros via `wsl -l -q`)
- Session sidebar grouped by project folder, with pins and archives
- Session restore from `%APPDATA%\ankar\sessions.json`
- Status detection: active / waiting / finished + Windows toasts when unfocused
- Configurable launchers on the bottom bar
- Command palette (`Ctrl+Shift+K`)
- Read-only project file viewer with syntect highlighting
- Zero network calls from the app itself

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

font_size = 14.0
accent = "#3A7CA5"

# Scrollback lines per terminal (default 3000; hard cap 50000)
scrollback_lines = 3000

pinned_projects = []
pinned_sessions = []

[[launchers]]
name = "opencode"
command = "opencode"
args = []

[[launchers]]
name = "cursor-agent"
command = "cursor-agent"
args = []
```

Session layout is stored in `%APPDATA%\ankar\sessions.json` (tabs, panes, folders, commands — not scrollback).

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

## Privacy

- No analytics, no accounts, no cloud sync
- No HTTP client in the dependency set used by the app
- Notifications are local Windows toasts only

## License

MIT
