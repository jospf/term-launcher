mod config;

use config::Config;
use std::env;
use std::fs;
use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::process::Command;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn sanitize_for_tui(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(stdout(), terminal::LeaveAlternateScreen, cursor::Show);
    }
}

fn allowed_bins() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/bin"),
    ];
    if let Ok(home) = env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/bin"));
    }
    dirs
}

#[cfg(unix)]
fn is_executable(meta: &fs::Metadata) -> bool {
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &fs::Metadata) -> bool {
    true
}

#[cfg(unix)]
fn dir_world_writable(dir: &Path) -> bool {
    if let Ok(meta) = fs::metadata(dir) {
        let mode = meta.permissions().mode();
        mode & 0o022 != 0
    } else {
        true
    }
}

#[cfg(not(unix))]
fn dir_world_writable(_dir: &Path) -> bool { false }

fn is_allowed_path(path: &Path) -> bool {
    if let Ok(canon) = fs::canonicalize(path) {
        for base in allowed_bins() {
            if let Ok(base_canon) = fs::canonicalize(base) {
                if canon.starts_with(&base_canon) {
                    return true;
                }
            }
        }
    }
    false
}

fn resolve_command(cmd: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(cmd);
    if candidate.is_absolute() {
        let meta = fs::metadata(&candidate).ok()?;
        if meta.is_file() && is_executable(&meta) && is_allowed_path(&candidate) {
            return fs::canonicalize(&candidate).ok();
        }
        return None;
    }

    let path_env = env::var("PATH").ok()?;
    for dir_str in path_env.split(':') {
        if dir_str.is_empty() { continue; }
        let dir = PathBuf::from(dir_str);
        if !dir.is_absolute() { continue; }
        if dir_world_writable(&dir) { continue; }
        let path = dir.join(cmd);
        if let Ok(meta) = fs::metadata(&path) {
            if meta.is_file() && is_executable(&meta) && is_allowed_path(&path) {
                if let Ok(canon) = fs::canonicalize(&path) {
                    return Some(canon);
                }
            }
        }
    }
    None
}

fn main() {
    // Load config
    let home = env::var("HOME").expect("No HOME env var found");
    let config_path = PathBuf::from(home).join(".config/term-launcher/config.toml");
    let config_contents = fs::read_to_string(config_path).expect("Failed to read config");
    let config: Config = toml::from_str(&config_contents).expect("Failed to parse config");

    // TUI setup with guard to ensure cleanup on panic/exit
    let mut stdout = stdout();
    let _guard = TerminalGuard::enter().expect("Failed to initialize terminal UI");

    let mut selected = 0;

    loop {
        // Clear screen and render menu
        if execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .is_err()
        {
            break;
        }
        if writeln!(stdout, "Term Launcher (↑ ↓ Enter to launch, q to quit)\n").is_err() {
            break;
        }

        for (i, app) in config.apps.iter().enumerate() {
            let y = (i + 2) as u16; // Offset to avoid header
            if execute!(
                stdout,
                cursor::MoveTo(0, y),
                terminal::Clear(ClearType::CurrentLine)
            )
            .is_err()
            {
                break;
            }

            let name = sanitize_for_tui(&app.name);
            let key = sanitize_for_tui(&app.key);
            if i == selected {
                if write!(stdout, "> {} ({})\n", name, key).is_err() { break; }
            } else {
                if write!(stdout, "  {} ({})\n", name, key).is_err() { break; }
            }
        }

        if stdout.flush().is_err() { break; }

        // Handle key events
        if let Ok(Event::Key(key_event)) = event::read() {
            match key_event.code {
                KeyCode::Char('q') => break,
                KeyCode::Up => {
                    if selected > 0 {
                        selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if !config.apps.is_empty() {
                        if selected + 1 < config.apps.len() { selected += 1; }
                    }
                }
                KeyCode::Enter => {
                    if config.apps.is_empty() {
                        continue;
                    }
                    let app = &config.apps[selected];

                    // Leave raw mode and screen for launching
                    let _ = terminal::disable_raw_mode();
                    let _ = execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show);

                    // Resolve command safely via PATH with allowlist
                    let resolved = resolve_command(&app.cmd);
                    if resolved.is_none() {
                        println!("Refusing to launch command: {}", app.cmd);
                        println!("Not found in allowed locations: /usr/bin, /usr/local/bin, /bin, ~/.local/bin");
                        println!("Provide absolute path or place binary in allowed dirs.");
                        println!("Press any key to return to the launcher...");
                        // Enable raw temporarily so any keypress is captured immediately
                        let _ = terminal::enable_raw_mode();
                        let _ = event::read();
                        // Re-enter raw mode and UI (keep the original guard alive)
                        let _ = terminal::enable_raw_mode();
                        let _ = execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide);
                        continue;
                    }
                    let resolved_cmd = resolved.unwrap();

                    // Launch the app
                    let mut command = Command::new(&resolved_cmd);
                    if let Some(args) = &app.args {
                        command.args(args);
                    }
                    let status = command.status();

                    match status {
                        Ok(status) => println!("\nProcess exited with status: {}\n", status),
                        Err(e) => println!("\nFailed to launch command: {}\n", e),
                    }
                    println!("Press any key to return to the launcher...");
                    // Enable raw temporarily so any keypress is captured immediately
                    let _ = terminal::enable_raw_mode();
                    let _ = event::read();

                    // Re-enter raw mode and UI (keep the original guard alive)
                    let _ = terminal::enable_raw_mode();
                    let _ = execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide);
                }
                _ => {}
            }
        }
    }

    // Final cleanup
    // TerminalGuard Drop will handle cleanup
}
