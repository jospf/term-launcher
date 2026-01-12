mod config;
mod launcher;

use config::{Config, App};
use std::env;
use std::fs;
use std::io::{self, Write, stdout};
use std::path::{PathBuf};
use std::process::Command;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{self, ClearType},
};

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

fn prompt(message: &str) -> io::Result<String> {
    print!("{}", message);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_with_default(label: &str, default: &str) -> io::Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let result = input.trim().to_string();
    if result.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(result)
    }
}

fn pause_with_message(msg: &str) -> io::Result<()> {
    println!("{}", msg);
    println!("Press any key to return...");
    terminal::enable_raw_mode()?;
    event::read()?;
    Ok(())
}

fn launch_app(app: &App) -> io::Result<()> {
    // Leave raw mode and screen for launching
    terminal::disable_raw_mode()?;
    execute!(stdout(), terminal::LeaveAlternateScreen, cursor::Show)?;

    let mut resolved_path = None;
    let mut final_args = app.args.clone();

    // 1. Try standard resolution
    if let Some(path) = launcher::resolve_command(&app.cmd) {
        resolved_path = Some(path);
    } 
    // 2. Fallback: Try splitting command by whitespace (legacy/malformed config support)
    else {
        let parts: Vec<&str> = app.cmd.split_whitespace().collect();
        if parts.len() > 1 {
            let base_cmd = parts[0];
            if let Some(path) = launcher::resolve_command(base_cmd) {
                resolved_path = Some(path);
                // Prepend implicit args found in cmd string
                let implicit_args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                let mut new_args = implicit_args;
                if let Some(existing) = final_args {
                    new_args.extend(existing);
                }
                final_args = Some(new_args);
            }
        }
    }

    if let Some(path) = resolved_path {
        // Launch the app
        let mut command = Command::new(&path);
        if let Some(args) = &final_args {
            command.args(args);
        }
        let status = command.status();

        match status {
            Ok(status) => println!("\nProcess exited with status: {}\n", status),
            Err(e) => println!("\nFailed to launch command: {}\n", e),
        }
        pause_with_message("")?;
    } else {
        println!("Refusing to launch command: {}", app.cmd);
        println!("Not found in allowed locations: /usr/bin, /usr/local/bin, /bin, ~/.local/bin");
        println!("Provide absolute path or place binary in allowed dirs.");
        pause_with_message("")?;
    }

    // Restore TUI
    terminal::enable_raw_mode()?;
    execute!(stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
    Ok(())
}

fn main() {
    // Load config
    let home = env::var("HOME").expect("No HOME env var found");
    let config_path = PathBuf::from(home).join(".config/term-launcher/config.toml");
    
    // Create config dir if not exists
    if let Some(parent) = config_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let config = if config_path.exists() {
        let config_contents = fs::read_to_string(&config_path).expect("Failed to read config");
        toml::from_str(&config_contents).expect("Failed to parse config")
    } else {
        Config { apps: vec![] }
    };

    // TUI setup with guard to ensure cleanup on panic/exit
    let _guard = TerminalGuard::enter().expect("Failed to initialize terminal UI");

    if let Err(e) = run_app(config, config_path) {
        eprintln!("Application error: {}", e);
    }
}

fn run_app(mut config: Config, config_path: PathBuf) -> io::Result<()> {
    let mut stdout = stdout();
    let mut selected = 0;

    loop {
        // Clamp selected
        if !config.apps.is_empty() && selected >= config.apps.len() {
            selected = config.apps.len() - 1;
        }

        // Clear screen and render menu
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        writeln!(stdout, "Term Launcher (↑ ↓ Enter to launch, Ctrl+a add, Ctrl+d del, Ctrl+e edit, Ctrl+q quit)\n")?;

        if config.apps.is_empty() {
             writeln!(stdout, "  No apps configured. Press 'Ctrl+a' to add one.")?;
        }

        for (i, app) in config.apps.iter().enumerate() {
            let y = (i + 2) as u16; // Offset to avoid header
            execute!(
                stdout,
                cursor::MoveTo(0, y),
                terminal::Clear(ClearType::CurrentLine)
            )?;

            let name = sanitize_for_tui(&app.name);
            let key = sanitize_for_tui(&app.key);
            if i == selected {
                write!(stdout, "> {} ({})\n", name, key)?;
            } else {
                write!(stdout, "  {} ({})\n", name, key)?;
            }
        }

        stdout.flush()?;

        // Handle key events
        if let Event::Key(key_event) = event::read()? {
            match (key_event.code, key_event.modifiers) {
                (KeyCode::Char('q'), KeyModifiers::CONTROL) => return Ok(()),
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    if !config.apps.is_empty() {
                        // Confirmation Step
                        terminal::disable_raw_mode()?;
                        execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;

                        let app_name = &config.apps[selected].name;
                        let answer = prompt(&format!("Are you sure you want to delete '{}'? (y/N): ", app_name))?;
                        
                        if answer.eq_ignore_ascii_case("y") {
                            config.apps.remove(selected);
                            config.save(&config_path)?;
                        }

                        // Restore
                        terminal::enable_raw_mode()?;
                        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
                    }
                }
                (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                    // Temporarily leave TUI
                    terminal::disable_raw_mode()?;
                    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;

                    println!("\n--- Add New App ---");
                    let name = prompt("Name: ")?;
                    let key = prompt("Key: ")?;

                    // Validation
                    // Note: We no longer have reserved single-char keys!
                    let key_exists = config.apps.iter().any(|app| app.key == key);

                    if key_exists {
                        pause_with_message(&format!("Error: Key '{}' is already in use by another app.", key))?;
                    } else if name.is_empty() || key.is_empty() {
                        pause_with_message("Error: Name and Key cannot be empty.")?;
                    } else {
                        let cmd_input = prompt("Command: ")?;
                        if !cmd_input.is_empty() {
                            let parts: Vec<&str> = cmd_input.split_whitespace().collect();
                            let cmd = parts[0].to_string();
                            let args = if parts.len() > 1 {
                                Some(parts[1..].iter().map(|s| s.to_string()).collect())
                            } else {
                                None
                            };

                            config.apps.push(App {
                                name,
                                key,
                                cmd,
                                args,
                            });
                            // Sort apps alphabetically by name
                            config.apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                            config.save(&config_path)?;
                        }
                    }

                    // Restore TUI
                    terminal::enable_raw_mode()?;
                    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
                }
                (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                     if !config.apps.is_empty() {
                        // Temporarily leave TUI
                        terminal::disable_raw_mode()?;
                        execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;

                        {
                            let app = &config.apps[selected];
                            println!("\n--- Edit App (Press Enter to keep current value) ---");
                            
                            // 1. Name
                            let new_name = prompt_with_default("Name", &app.name)?;
                            
                            // 2. Key
                            let new_key = prompt_with_default("Key", &app.key)?;
                            
                            // 3. Command
                            let mut full_cmd_str = app.cmd.clone();
                            if let Some(args) = &app.args {
                                full_cmd_str.push_str(" ");
                                full_cmd_str.push_str(&args.join(" "));
                            }
                            
                            let new_cmd_input = prompt_with_default("Command", &full_cmd_str)?;

                            // Validation
                            // Check if key exists (excluding THIS app's current key if it hasn't changed)
                            let key_conflict = config.apps.iter().enumerate().any(|(i, check_app)| {
                                i != selected && check_app.key == new_key
                            });

                            if key_conflict {
                                pause_with_message(&format!("Error: Key '{}' is already in use by another app.", new_key))?;
                            } else if new_name.is_empty() || new_key.is_empty() || new_cmd_input.is_empty() {
                                pause_with_message("Error: Fields cannot be empty.")?;
                            } else {
                                // Parse the new command string
                                let parts: Vec<&str> = new_cmd_input.split_whitespace().collect();
                                let cmd = parts[0].to_string();
                                let args = if parts.len() > 1 {
                                    Some(parts[1..].iter().map(|s| s.to_string()).collect())
                                } else {
                                    None
                                };

                                // Update
                                config.apps[selected] = App {
                                    name: new_name,
                                    key: new_key,
                                    cmd,
                                    args,
                                };
                                // Sort
                                config.apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                config.save(&config_path)?;
                            }
                        }

                        // Restore TUI
                        terminal::enable_raw_mode()?;
                        execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;
                    }
                }
                (KeyCode::Up, _) => {
                    if selected > 0 {
                        selected -= 1;
                    }
                }
                (KeyCode::Down, _) => {
                    if !config.apps.is_empty() {
                        if selected + 1 < config.apps.len() { selected += 1; }
                    }
                }
                (KeyCode::Enter, _) => {
                    if !config.apps.is_empty() {
                        let app = &config.apps[selected];
                        launch_app(app)?;
                    }
                }
                (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                    // Check if any app has this key
                    if let Some(app) = config.apps.iter().find(|a| a.key == c.to_string()) {
                         launch_app(app)?;
                    }
                }
                _ => {}
            }
        }
    }
}
