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
    style::{self, Color},
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

        // Clear screen
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::Hide
        )?;

        let (term_cols, term_rows) = terminal::size()?;
        
        // Calculate menu dimensions
        // Box Width: at least 80, max 80% of screen
        let box_width = std::cmp::max(80, (term_cols as f32 * 0.8) as u16);
        let box_height = std::cmp::max(12, (term_rows as f32 * 0.6) as u16);
        
        let start_x = (term_cols.saturating_sub(box_width)) / 2;
        let start_y = (term_rows.saturating_sub(box_height)) / 2;

        let inner_width = box_width.saturating_sub(2);
        let left_pane_width = (inner_width as f32 * 0.55) as u16;
        let right_pane_width = inner_width.saturating_sub(left_pane_width).saturating_sub(1);
        let divider_x = start_x + 1 + left_pane_width;

        // Draw Border
        execute!(stdout, style::SetForegroundColor(Color::Blue))?;
        
        // Top
        let left_top_dashes = "─".repeat(left_pane_width as usize);
        let right_top_dashes = "─".repeat(right_pane_width as usize);
        execute!(stdout, cursor::MoveTo(start_x, start_y))?;
        write!(stdout, "╭{}┬{}╮", left_top_dashes, right_top_dashes)?;

        // Sides and divider
        let left_spaces = " ".repeat(left_pane_width as usize);
        let right_spaces = " ".repeat(right_pane_width as usize);
        for i in 1..box_height.saturating_sub(1) {
            execute!(stdout, cursor::MoveTo(start_x, start_y + i))?;
            write!(stdout, "│{}│{}│", left_spaces, right_spaces)?;
        }

        // Bottom
        let left_bottom_dashes = "─".repeat(left_pane_width as usize);
        let right_bottom_dashes = "─".repeat(right_pane_width as usize);
        execute!(stdout, cursor::MoveTo(start_x, start_y + box_height - 1))?;
        write!(stdout, "╰{}┴{}╯", left_bottom_dashes, right_bottom_dashes)?;
        
        execute!(stdout, style::ResetColor)?;

        // Title
        let title = " Term Launcher ";
        let title_start_x = start_x + 1 + (left_pane_width.saturating_sub(title.len() as u16)) / 2;
        execute!(stdout, cursor::MoveTo(title_start_x, start_y), style::SetForegroundColor(Color::Yellow), style::SetAttribute(style::Attribute::Bold))?;
        write!(stdout, "{}", title)?;
        execute!(stdout, style::ResetColor)?;

        // Help Text (Left bottom border)
        let left_help = " Ctrl+a:Add  Ctrl+d:Del  Ctrl+e:Edit ";
        let left_help_x = start_x + 1 + (left_pane_width.saturating_sub(left_help.len() as u16)) / 2;
        execute!(stdout, cursor::MoveTo(left_help_x, start_y + box_height - 1), style::SetForegroundColor(Color::DarkGrey))?;
        write!(stdout, "{}", left_help)?;
        
        // Help Text (Right bottom border)
        let right_help = " Ctrl+q:Quit ";
        let right_help_x = divider_x + 1 + (right_pane_width.saturating_sub(right_help.len() as u16)) / 2;
        execute!(stdout, cursor::MoveTo(right_help_x, start_y + box_height - 1), style::SetForegroundColor(Color::DarkGrey))?;
        write!(stdout, "{}", right_help)?;
        execute!(stdout, style::ResetColor)?;

        // Content Area
        let content_start_y = start_y + 2;
        let max_items = box_height.saturating_sub(4) as usize; // Check math: 2 top padding + 2 bottom padding

        // Scroll logic (basic)
        let start_index = if selected >= max_items {
            selected - max_items + 1
        } else {
            0
        };
        let end_index = std::cmp::min(config.apps.len(), start_index + max_items);

        let display_apps = &config.apps[start_index..end_index];

        if config.apps.is_empty() {
             let msg = "No apps configured.";
             let msg_x = start_x + 1 + (left_pane_width.saturating_sub(msg.len() as u16)) / 2;
             execute!(stdout, cursor::MoveTo(msg_x, content_start_y), style::SetForegroundColor(Color::DarkGrey))?;
             write!(stdout, "{}", msg)?;
             execute!(stdout, style::ResetColor)?;
        }

        for (i, app) in display_apps.iter().enumerate() {
            let actual_idx = start_index + i;
            let row = content_start_y + i as u16;
            
            // Format line: "Name (Key)"
            // Truncate if too long
            let left_inner_width = left_pane_width.saturating_sub(4);
            let key_str = format!("({})", sanitize_for_tui(&app.key));
            let name_str = sanitize_for_tui(&app.name);
            
            let mut line = format!("{} {}", name_str, key_str);
            if line.len() > left_inner_width as usize {
                line.truncate(left_inner_width as usize);
            }

            let line_start_x = start_x + 1 + (left_pane_width.saturating_sub(line.len() as u16)) / 2;

            if actual_idx == selected {
                // Highlight selected row
                let marked_line = format!("> {} <", line);
                let marked_start_x = start_x + 1 + (left_pane_width.saturating_sub(marked_line.len() as u16)) / 2;
                
                execute!(stdout, cursor::MoveTo(marked_start_x, row), style::SetForegroundColor(Color::Black), style::SetBackgroundColor(Color::Cyan))?;
                write!(stdout, "{}", marked_line)?;
                execute!(stdout, style::ResetColor)?;
            } else {
                execute!(stdout, cursor::MoveTo(line_start_x, row))?;
                write!(stdout, "{}", line)?;
            }
        }    

        // Draw Right Pane Details
        if !config.apps.is_empty() {
            let app = &config.apps[selected];
            let right_x = divider_x + 2;
            let inner_r_width = right_pane_width.saturating_sub(4) as usize;
            
            let mut r_row = content_start_y;

            // 1. Draw Title (done first, before closure limits borrow access)
            let details_title = " App Details ";
            let details_title_x = divider_x + 1 + (right_pane_width.saturating_sub(details_title.len() as u16)) / 2;
            execute!(stdout, cursor::MoveTo(details_title_x, start_y + 1), style::SetForegroundColor(Color::Cyan), style::SetAttribute(style::Attribute::Bold))?;
            write!(stdout, "{}", details_title)?;
            execute!(stdout, style::ResetColor)?;

            let mut draw_detail_line = |stdout: &mut io::Stdout, label: &str, value: &str, label_color: Color, val_color: Color| -> io::Result<()> {
                if r_row >= start_y + box_height - 1 {
                    return Ok(());
                }
                execute!(stdout, cursor::MoveTo(right_x, r_row))?;
                
                let label_part = format!("{}: ", label);
                let available_val_width = inner_r_width.saturating_sub(label_part.len());
                let mut val_part = value.to_string();
                if val_part.len() > available_val_width {
                    val_part.truncate(available_val_width);
                }
                
                execute!(stdout, style::SetForegroundColor(label_color))?;
                write!(stdout, "{}", label_part)?;
                execute!(stdout, style::SetForegroundColor(val_color))?;
                write!(stdout, "{}", val_part)?;
                execute!(stdout, style::ResetColor)?;
                
                r_row += 1;
                Ok(())
            };

            // 2. Name
            draw_detail_line(&mut stdout, "Name", &app.name, Color::Yellow, Color::White)?;

            // 3. Hotkey
            draw_detail_line(&mut stdout, "Hotkey", &app.key, Color::Yellow, Color::White)?;

            // 4. Command
            draw_detail_line(&mut stdout, "Command", &app.cmd, Color::Yellow, Color::White)?;

            // 5. Resolved Path
            let path_resolved = launcher::resolve_command(&app.cmd);
            let (path_str, path_color) = if let Some(path) = path_resolved {
                (path.to_string_lossy().into_owned(), Color::Green)
            } else {
                ("Not found / Blocked".to_string(), Color::Red)
            };
            draw_detail_line(&mut stdout, "Resolved", &path_str, Color::Yellow, path_color)?;

            // 6. Arguments
            let args_str = match &app.args {
                Some(args) if !args.is_empty() => args.join(" "),
                _ => "None".to_string(),
            };
            draw_detail_line(&mut stdout, "Args", &args_str, Color::Yellow, Color::White)?;

            // 7. Description
            let desc_str = app.description.as_deref().unwrap_or("No description provided");
            let desc_color = if app.description.is_some() { Color::White } else { Color::DarkGrey };
            draw_detail_line(&mut stdout, "Desc", desc_str, Color::Yellow, desc_color)?;
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
                            let description_input = prompt("Description (optional): ")?;
                            let description = if description_input.is_empty() {
                                None
                            } else {
                                Some(description_input)
                            };

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
                                description,
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

                            let current_desc = app.description.as_deref().unwrap_or("");
                            let new_desc_input = prompt_with_default("Description", current_desc)?;
                            let description = if new_desc_input.is_empty() {
                                None
                            } else {
                                Some(new_desc_input)
                            };

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
                                    description,
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
