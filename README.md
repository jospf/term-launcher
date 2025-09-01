Term Launcher – a tiny TUI to launch apps from a config file.

**Quick Start**
- Build/run: `cargo run`
- Config path: `$HOME/.config/term-launcher/config.toml`
- Keys: Up/Down to select, Enter to launch, `q` to quit

**Configuration** (`$HOME/.config/term-launcher/config.toml`)
- Top-level: `apps` array
- Each app:
  - `name`: display name (sanitized for TUI)
  - `key`: short hint shown next to the app
  - `cmd`: program to execute (absolute path or found on PATH)
  - `args` (optional): array of arguments, no shell parsing

Example:

```toml
[[apps]]
name = "Yazi"
key  = "y"
cmd  = "yazi"                # or "/usr/bin/yazi"
args = []

[[apps]]
name = "htop"
key  = "h"
cmd  = "/usr/bin/htop"
```

**Security Model**
- No shell: commands are executed directly via `Command::new` with optional `.args`, never through `sh -c`.
- PATH allowlist: non-absolute `cmd` is resolved only from these directories: `/usr/bin`, `/usr/local/bin`, `/bin`, `$HOME/.local/bin`.
- Executable check: binaries must exist and be executable; symlinks are canonicalized.
- TUI safety: control characters are stripped from `name`/`key` before rendering.
- Terminal reliability: raw mode/alternate screen are restored even on errors.

If a command cannot be resolved or is outside allowed locations, the launcher refuses to start it and explains why.

**Troubleshooting**
- “Refusing to launch command …”: ensure the program is either referenced by absolute path or is on PATH under one of the allowlisted directories.
- Nothing happens after program exits: press any key to return (raw mode is enabled for this prompt).
- Empty app list: the UI will show nothing selectable; add entries to the config.

**Notes**
- The launcher is designed for user contexts, not privileged execution. Avoid running it as root.
