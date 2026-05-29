Term Launcher – a tiny, highly customizable TUI to launch apps from a config file.

**Quick Start**
- Build/run: `cargo run`
- Config path: `$HOME/.config/term-launcher/config.toml`
- Hotkeys:
  - `Up/Down` to select an application
  - `Enter` to launch the selected application
  - `/` to activate dynamic search/filtering
  - `Ctrl+a` to stage and **Add** a new application
  - `Ctrl+e` to **Edit** the selected application
  - `Ctrl+d` to **Delete** the selected application
  - `Ctrl+t` to open the **Theme Selector** modal
  - `Ctrl+q` to quit the launcher

**Configuration** (`$HOME/.config/term-launcher/config.toml`)
- Top-level fields:
  - `apps`: Array of applications to list.
  - `theme` (optional): Styling configuration.
- Each app:
  - `name`: Display name (sanitized for TUI)
  - `key`: Hotkey character shown next to the app
  - `cmd`: Program to execute (absolute path or found on PATH allowlist)
  - `args` (optional): Array of arguments (no shell parsing/expansion)
  - `description` (optional): Descriptive label shown in inspector details
- The `theme` table:
  - `accent_color` (optional): Interactive elements, highlighting, matching text, active form borders (e.g., `"cyan"`, `"magenta"`, `"yellow"`)
  - `border_color` (optional): Outer panels and divider borders
  - `text_color` (optional): Default foreground text
  - `dim_color` (optional): Unselected hints, hotkey cues, and help instructions

Example Config:

```toml
[[apps]]
name = "Yazi"
key  = "y"
cmd  = "yazi"                # or "/usr/bin/yazi"
args = []
description = "Terminal File Manager"

[[apps]]
name = "htop"
key  = "h"
cmd  = "/usr/bin/htop"
description = "System Monitor"

[theme]
border_color = "dark_cyan"
accent_color = "cyan"
text_color = "grey"
dim_color = "dark_grey"
```

**Predefined Themes**
Press **`Ctrl+T`** within the app to dynamically pick and swap between these beautiful presets:
1. **Default Blue**: Balanced high-contrast corporate look.
2. **Cyberpunk Neon**: Retro neon pink/magenta and cyan highlights.
3. **Nordic Frost**: Muted cyan and grey arctic visual theme.
4. **Gruvbox Autumn**: Warm brown/golden tones.
5. **Dracula Night**: Deep dark purple and vibrant magenta.
6. **Matrix Terminal**: Retro green hacker design.
7. **Sunset Crimson**: Fiery dark red and golden highlights.

**Security Model**
- **No shell**: Commands are executed directly via `Command::new` with optional `.args`, never through `sh -c` or shell environment contexts.
- **PATH allowlist**: Non-absolute `cmd` is resolved only from allowed directories: `/usr/bin`, `/usr/local/bin`, `/bin`, and `$HOME/.local/bin`.
- **Executable check**: Binaries must exist and be executable; symlinks are canonicalized.
- **TUI safety**: Control characters are stripped from `name`/`key` before rendering.
- **Terminal reliability**: Raw mode/alternate screen are safely restored even on unexpected crashes.

If a command cannot be resolved or resides outside allowed locations, the launcher refuses to start it and explains why.

**Troubleshooting**
- “Refusing to launch command …”: ensure the program is either referenced by an absolute path or is in a directory on the PATH allowlist.
- Nothing happens after program exits: press any key to return (raw mode is enabled for this prompt).
- Empty app list: the UI will show nothing selectable; add entries using `Ctrl+a` or add them manually to the config.

**Notes**
- The launcher is designed for user contexts, not privileged execution. Avoid running it as root.
