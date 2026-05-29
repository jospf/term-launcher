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

#[derive(Clone, Debug)]
struct FormField {
    label: &'static str,
    value: String,
    cursor_pos: usize,
}

#[derive(Clone, Debug)]
struct FormState {
    title: &'static str,
    fields: Vec<FormField>,
    active_field: usize,
    error_message: Option<String>,
    is_edit: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum ModalState {
    None,
    Form,
    DeleteConfirm,
}

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

    let mut modal_state = ModalState::None;
    let mut active_form: Option<FormState> = None;

    let mut search_query = String::new();
    let mut search_active = false;
    let mut search_cursor_pos = 0;

    loop {
        // Filter apps dynamically
        let filtered_apps: Vec<&App> = config.apps.iter()
            .filter(|app| {
                search_query.is_empty() || 
                app.name.to_lowercase().contains(&search_query.to_lowercase())
            })
            .collect();

        // Clamp selected
        if !filtered_apps.is_empty() && selected >= filtered_apps.len() {
            selected = filtered_apps.len() - 1;
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

        // Sides and divider background
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

        // Draw horizontal divider under search bar
        execute!(stdout, style::SetForegroundColor(Color::Blue))?;
        execute!(stdout, cursor::MoveTo(start_x, start_y + 3))?;
        write!(stdout, "├{}┼", "─".repeat(left_pane_width as usize))?;
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

        // Draw Search Bar
        let search_label = " 🔎 Search: ";
        let search_y = start_y + 2;
        execute!(stdout, cursor::MoveTo(start_x + 2, search_y))?;
        if search_active {
            execute!(stdout, style::SetForegroundColor(Color::Cyan), style::SetAttribute(style::Attribute::Bold))?;
        } else {
            execute!(stdout, style::SetForegroundColor(Color::Yellow))?;
        }
        write!(stdout, "{}", search_label)?;
        execute!(stdout, style::ResetColor)?;

        // Search Input box
        execute!(stdout, cursor::MoveTo(start_x + 12, search_y))?;
        if search_active {
            execute!(stdout, style::SetForegroundColor(Color::Cyan))?;
        } else {
            execute!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
        }
        write!(stdout, "[")?;
        
        execute!(stdout, cursor::MoveTo(start_x + 13, search_y), style::SetForegroundColor(Color::White))?;
        let search_inner_width = left_pane_width.saturating_sub(15);
        let mut display_search = search_query.clone();
        if display_search.len() > search_inner_width as usize {
            display_search.truncate(search_inner_width as usize);
        }
        write!(stdout, "{}", display_search)?;
        // Pad spaces
        let spaces = search_inner_width.saturating_sub(display_search.len() as u16);
        write!(stdout, "{}", " ".repeat(spaces as usize))?;

        if search_active {
            execute!(stdout, style::SetForegroundColor(Color::Cyan))?;
        } else {
            execute!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
        }
        write!(stdout, "]")?;
        execute!(stdout, style::ResetColor)?;

        // Content Area (List starts below search divider)
        let content_start_y = start_y + 4;
        let max_items = box_height.saturating_sub(6) as usize;

        // Scroll logic (basic)
        let start_index = if selected >= max_items {
            selected - max_items + 1
        } else {
            0
        };
        let end_index = std::cmp::min(filtered_apps.len(), start_index + max_items);

        let display_apps = &filtered_apps[start_index..end_index];

        if filtered_apps.is_empty() {
             let msg = "No apps found.";
             let msg_x = start_x + 1 + (left_pane_width.saturating_sub(msg.len() as u16)) / 2;
             execute!(stdout, cursor::MoveTo(msg_x, content_start_y), style::SetForegroundColor(Color::DarkGrey))?;
             write!(stdout, "{}", msg)?;
             execute!(stdout, style::ResetColor)?;
        }

        for (i, app) in display_apps.iter().enumerate() {
            let actual_idx = start_index + i;
            let row = content_start_y + i as u16;
            
            // Format name and key
            let key_str = format!("({})", sanitize_for_tui(&app.key));
            let name_str = sanitize_for_tui(&app.name);
            
            let line_start_x = start_x + 1 + (left_pane_width.saturating_sub((name_str.len() + 1 + key_str.len()) as u16)) / 2;
            
            if actual_idx == selected {
                // Selected: highlight with cyan background
                let line = format!("{} {}", name_str, key_str);
                let marked_line = format!("> {} <", line);
                let marked_start_x = start_x + 1 + (left_pane_width.saturating_sub(marked_line.len() as u16)) / 2;
                
                execute!(stdout, cursor::MoveTo(marked_start_x, row), style::SetForegroundColor(Color::Black), style::SetBackgroundColor(Color::Cyan))?;
                write!(stdout, "{}", marked_line)?;
                execute!(stdout, style::ResetColor)?;
            } else {
                // Not selected: substring highlight
                execute!(stdout, cursor::MoveTo(line_start_x, row))?;
                let mut match_found = false;
                if !search_query.is_empty() {
                    if let Some(pos) = name_str.to_lowercase().find(&search_query.to_lowercase()) {
                        match_found = true;
                        let prefix = &name_str[..pos];
                        let matched = &name_str[pos..pos + search_query.len()];
                        let suffix = &name_str[pos + search_query.len()..];

                        execute!(stdout, style::SetForegroundColor(Color::White))?;
                        write!(stdout, "{}", prefix)?;
                        
                        execute!(stdout, style::SetForegroundColor(Color::Magenta), style::SetAttribute(style::Attribute::Bold))?;
                        write!(stdout, "{}", matched)?;
                        
                        execute!(stdout, style::SetForegroundColor(Color::White), style::SetAttribute(style::Attribute::Reset))?;
                        write!(stdout, "{}", suffix)?;
                    }
                }
                
                if !match_found {
                    execute!(stdout, style::SetForegroundColor(Color::White))?;
                    write!(stdout, "{}", name_str)?;
                }

                execute!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
                write!(stdout, " {}", key_str)?;
                execute!(stdout, style::ResetColor)?;
            }
        }    

        // Draw Right Pane Details
        if !filtered_apps.is_empty() {
            let app = filtered_apps[selected];
            let right_x = divider_x + 2;
            let inner_r_width = right_pane_width.saturating_sub(4) as usize;
            
            let mut r_row = content_start_y;

            // 1. Draw Title
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

        // Draw Form Modal Overlay
        if modal_state == ModalState::Form {
            if let Some(ref form) = active_form {
                let modal_width = 60;
                let modal_height = 14;
                let modal_x = (term_cols.saturating_sub(modal_width)) / 2;
                let modal_y = (term_rows.saturating_sub(modal_height)) / 2;

                execute!(stdout, style::SetForegroundColor(Color::Magenta))?;
                
                // Top
                let title_bar = format!(" {} ", form.title);
                let dash_len = (modal_width as usize - 2 - title_bar.len()) / 2;
                let left_dashes = "═".repeat(dash_len);
                let right_dashes = "═".repeat(modal_width as usize - 2 - title_bar.len() - dash_len);
                execute!(stdout, cursor::MoveTo(modal_x, modal_y))?;
                write!(stdout, "╔{}{}{}╗", left_dashes, title_bar, right_dashes)?;

                // Sides
                for r in 1..modal_height - 1 {
                    execute!(stdout, cursor::MoveTo(modal_x, modal_y + r))?;
                    write!(stdout, "║{}║", " ".repeat((modal_width - 2) as usize))?;
                }

                // Bottom
                execute!(stdout, cursor::MoveTo(modal_x, modal_y + modal_height - 1))?;
                write!(stdout, "╚{}╝", "═".repeat((modal_width - 2) as usize))?;
                execute!(stdout, style::ResetColor)?;

                // Draw fields
                for (idx, field) in form.fields.iter().enumerate() {
                    let field_y = modal_y + 3 + (2 * idx) as u16;
                    
                    // Label
                    execute!(stdout, cursor::MoveTo(modal_x + 3, field_y))?;
                    if idx == form.active_field {
                        execute!(stdout, style::SetForegroundColor(Color::Cyan), style::SetAttribute(style::Attribute::Bold))?;
                    } else {
                        execute!(stdout, style::SetForegroundColor(Color::Yellow))?;
                    }
                    write!(stdout, "{:11}", field.label)?;
                    execute!(stdout, style::ResetColor)?;

                    // Input bracket
                    execute!(stdout, cursor::MoveTo(modal_x + 15, field_y))?;
                    if idx == form.active_field {
                        execute!(stdout, style::SetForegroundColor(Color::Cyan))?;
                    } else {
                        execute!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
                    }
                    write!(stdout, "[")?;
                    
                    // Value
                    execute!(stdout, cursor::MoveTo(modal_x + 16, field_y), style::SetForegroundColor(Color::White))?;
                    let val_limit = 39;
                    let mut display_val = field.value.clone();
                    if display_val.len() > val_limit {
                        display_val.truncate(val_limit);
                    }
                    write!(stdout, "{}", display_val)?;

                    // Fill remaining input box space
                    let spaces = val_limit.saturating_sub(display_val.len());
                    write!(stdout, "{}", " ".repeat(spaces))?;

                    // Close bracket
                    if idx == form.active_field {
                        execute!(stdout, style::SetForegroundColor(Color::Cyan))?;
                    } else {
                        execute!(stdout, style::SetForegroundColor(Color::DarkGrey))?;
                    }
                    write!(stdout, "]")?;
                    execute!(stdout, style::ResetColor)?;
                }

                // Draw buttons/help in modal
                let form_help = " [Enter] Save   [Esc] Cancel   [Tab] Next ";
                let form_help_x = modal_x + (modal_width.saturating_sub(form_help.len() as u16)) / 2;
                execute!(stdout, cursor::MoveTo(form_help_x, modal_y + 11), style::SetForegroundColor(Color::DarkGrey))?;
                write!(stdout, "{}", form_help)?;
                execute!(stdout, style::ResetColor)?;

                // Draw error message if any
                if let Some(ref err) = form.error_message {
                    let err_display = format!("Error: {}", err);
                    let err_x = modal_x + (modal_width.saturating_sub(err_display.len() as u16)) / 2;
                    execute!(stdout, cursor::MoveTo(err_x, modal_y + 12), style::SetForegroundColor(Color::Red), style::SetAttribute(style::Attribute::Bold))?;
                    write!(stdout, "{}", err_display)?;
                    execute!(stdout, style::ResetColor)?;
                }
            }
        }

        // Draw Delete Confirmation Modal Overlay
        if modal_state == ModalState::DeleteConfirm {
            if !filtered_apps.is_empty() {
                let app = filtered_apps[selected];
                let modal_width = 50;
                let modal_height = 8;
                let modal_x = (term_cols.saturating_sub(modal_width)) / 2;
                let modal_y = (term_rows.saturating_sub(modal_height)) / 2;

                execute!(stdout, style::SetForegroundColor(Color::Red))?;
                
                // Top
                let title = " Confirm Delete ";
                let dash_len = (modal_width as usize - 2 - title.len()) / 2;
                let left_dashes = "═".repeat(dash_len);
                let right_dashes = "═".repeat(modal_width as usize - 2 - title.len() - dash_len);
                execute!(stdout, cursor::MoveTo(modal_x, modal_y))?;
                write!(stdout, "╔{}{}{}╗", left_dashes, title, right_dashes)?;

                // Sides
                for r in 1..modal_height - 1 {
                    execute!(stdout, cursor::MoveTo(modal_x, modal_y + r))?;
                    write!(stdout, "║{}║", " ".repeat((modal_width - 2) as usize))?;
                }

                // Bottom
                execute!(stdout, cursor::MoveTo(modal_x, modal_y + modal_height - 1))?;
                write!(stdout, "╚{}╝", "═".repeat((modal_width - 2) as usize))?;
                execute!(stdout, style::ResetColor)?;

                // Message
                let msg1 = "Are you sure you want to delete";
                let msg2 = format!("'{}'?", app.name);
                let msg1_x = modal_x + (modal_width.saturating_sub(msg1.len() as u16)) / 2;
                let msg2_x = modal_x + (modal_width.saturating_sub(msg2.len() as u16)) / 2;
                
                execute!(stdout, cursor::MoveTo(msg1_x, modal_y + 2), style::SetForegroundColor(Color::White))?;
                write!(stdout, "{}", msg1)?;
                execute!(stdout, cursor::MoveTo(msg2_x, modal_y + 3), style::SetForegroundColor(Color::Yellow))?;
                write!(stdout, "{}", msg2)?;

                // Buttons
                let btn_help = " [y] Yes      [n/Esc] No ";
                let btn_x = modal_x + (modal_width.saturating_sub(btn_help.len() as u16)) / 2;
                execute!(stdout, cursor::MoveTo(btn_x, modal_y + 5), style::SetForegroundColor(Color::White))?;
                write!(stdout, "{}", btn_help)?;
                execute!(stdout, style::ResetColor)?;
            }
        }

        // Show/Hide Caret Cursor dynamically
        let mut show_cursor = false;
        let mut cursor_x = 0;
        let mut cursor_y = 0;

        if search_active {
            show_cursor = true;
            cursor_x = start_x + 13 + search_cursor_pos as u16;
            cursor_y = start_y + 2;
        } else if modal_state == ModalState::Form {
            if let Some(ref form) = active_form {
                show_cursor = true;
                let modal_width = 60;
                let modal_x = (term_cols.saturating_sub(modal_width)) / 2;
                let modal_y = (term_rows.saturating_sub(14)) / 2;
                
                cursor_y = modal_y + 3 + (2 * form.active_field) as u16;
                let active_field_state = &form.fields[form.active_field];
                cursor_x = modal_x + 16 + active_field_state.cursor_pos as u16;
            }
        }

        if show_cursor {
            execute!(stdout, cursor::MoveTo(cursor_x, cursor_y), cursor::Show)?;
        } else {
            execute!(stdout, cursor::Hide)?;
        }

        stdout.flush()?;

        // Handle key events
        if let Event::Key(key_event) = event::read()? {
            match modal_state {
                ModalState::DeleteConfirm => {
                    match key_event.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if !filtered_apps.is_empty() {
                                let app_to_delete = filtered_apps[selected];
                                if let Some(idx) = config.apps.iter().position(|a| a.name == app_to_delete.name && a.key == app_to_delete.key) {
                                    config.apps.remove(idx);
                                    let _ = config.save(&config_path);
                                }
                            }
                            modal_state = ModalState::None;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            modal_state = ModalState::None;
                        }
                        _ => {}
                    }
                }
                ModalState::Form => {
                    if let Some(ref mut form) = active_form {
                        match key_event.code {
                            KeyCode::Esc => {
                                modal_state = ModalState::None;
                                active_form = None;
                            }
                            KeyCode::Tab | KeyCode::Down => {
                                form.active_field = (form.active_field + 1) % form.fields.len();
                            }
                            KeyCode::BackTab | KeyCode::Up => {
                                form.active_field = (form.active_field + form.fields.len() - 1) % form.fields.len();
                            }
                            KeyCode::Left => {
                                let field = &mut form.fields[form.active_field];
                                if field.cursor_pos > 0 {
                                    field.cursor_pos -= 1;
                                }
                            }
                            KeyCode::Right => {
                                let field = &mut form.fields[form.active_field];
                                if field.cursor_pos < field.value.len() {
                                    field.cursor_pos += 1;
                                }
                            }
                            KeyCode::Backspace => {
                                let field = &mut form.fields[form.active_field];
                                if field.cursor_pos > 0 {
                                    field.value.remove(field.cursor_pos - 1);
                                    field.cursor_pos -= 1;
                                }
                            }
                            KeyCode::Delete => {
                                let field = &mut form.fields[form.active_field];
                                if field.cursor_pos < field.value.len() {
                                    field.value.remove(field.cursor_pos);
                                }
                            }
                            KeyCode::Char(c) => {
                                let field = &mut form.fields[form.active_field];
                                if field.value.len() < 39 {
                                    field.value.insert(field.cursor_pos, c);
                                    field.cursor_pos += 1;
                                }
                            }
                            KeyCode::Enter => {
                                let name = form.fields[0].value.trim().to_string();
                                let key = form.fields[1].value.trim().to_string();
                                let cmd_input = form.fields[2].value.trim().to_string();
                                let desc_input = form.fields[3].value.trim().to_string();

                                if name.is_empty() || key.is_empty() {
                                    form.error_message = Some("Name and Key cannot be empty.".to_string());
                                } else if cmd_input.is_empty() {
                                    form.error_message = Some("Command cannot be empty.".to_string());
                                } else {
                                    let mut key_conflict = false;
                                    if form.is_edit {
                                        if !filtered_apps.is_empty() {
                                            let current_app = filtered_apps[selected];
                                            key_conflict = config.apps.iter().any(|app| {
                                                app.key == key && (app.name != current_app.name || app.key != current_app.key)
                                            });
                                        }
                                    } else {
                                        key_conflict = config.apps.iter().any(|app| app.key == key);
                                    }

                                    if key_conflict {
                                        form.error_message = Some(format!("Key '{}' is already in use.", key));
                                    } else {
                                        let parts: Vec<&str> = cmd_input.split_whitespace().collect();
                                        let cmd = parts[0].to_string();
                                        let args = if parts.len() > 1 {
                                            Some(parts[1..].iter().map(|s| s.to_string()).collect())
                                        } else {
                                            None
                                        };

                                        let description = if desc_input.is_empty() {
                                            None
                                        } else {
                                            Some(desc_input)
                                        };

                                        if form.is_edit {
                                            if !filtered_apps.is_empty() {
                                                let current_app = filtered_apps[selected];
                                                if let Some(idx) = config.apps.iter().position(|a| a.name == current_app.name && a.key == current_app.key) {
                                                    config.apps[idx] = App {
                                                        name,
                                                        key,
                                                        cmd,
                                                        args,
                                                        description,
                                                    };
                                                }
                                            }
                                        } else {
                                            config.apps.push(App {
                                                name,
                                                key,
                                                cmd,
                                                args,
                                                description,
                                            });
                                        }

                                        config.apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                        if let Err(e) = config.save(&config_path) {
                                            form.error_message = Some(format!("Failed to save: {}", e));
                                        } else {
                                            modal_state = ModalState::None;
                                            active_form = None;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                ModalState::None => {
                    if search_active {
                        match key_event.code {
                            KeyCode::Esc => {
                                search_active = false;
                                search_query.clear();
                                search_cursor_pos = 0;
                            }
                            KeyCode::Enter | KeyCode::Down | KeyCode::Tab => {
                                search_active = false;
                            }
                            KeyCode::Left => {
                                if search_cursor_pos > 0 {
                                    search_cursor_pos -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if search_cursor_pos < search_query.len() {
                                    search_cursor_pos += 1;
                                }
                            }
                            KeyCode::Backspace => {
                                if search_cursor_pos > 0 {
                                    search_query.remove(search_cursor_pos - 1);
                                    search_cursor_pos -= 1;
                                }
                            }
                            KeyCode::Delete => {
                                if search_cursor_pos < search_query.len() {
                                    search_query.remove(search_cursor_pos);
                                }
                            }
                            KeyCode::Char(c) => {
                                search_query.insert(search_cursor_pos, c);
                                search_cursor_pos += 1;
                            }
                            _ => {}
                        }
                    } else {
                        match (key_event.code, key_event.modifiers) {
                            (KeyCode::Char('q'), KeyModifiers::CONTROL) => return Ok(()),
                            (KeyCode::Char('/'), KeyModifiers::NONE) => {
                                search_active = true;
                                search_cursor_pos = search_query.len();
                            }
                            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                                if !filtered_apps.is_empty() {
                                    modal_state = ModalState::DeleteConfirm;
                                }
                            }
                            (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                                modal_state = ModalState::Form;
                                active_form = Some(FormState {
                                    title: "Add New Application",
                                    fields: vec![
                                        FormField { label: "Name", value: String::new(), cursor_pos: 0 },
                                        FormField { label: "Hotkey", value: String::new(), cursor_pos: 0 },
                                        FormField { label: "Command", value: String::new(), cursor_pos: 0 },
                                        FormField { label: "Description", value: String::new(), cursor_pos: 0 },
                                    ],
                                    active_field: 0,
                                    error_message: None,
                                    is_edit: false,
                                });
                            }
                            (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                                if !filtered_apps.is_empty() {
                                    let app = filtered_apps[selected];
                                    let mut full_cmd_str = app.cmd.clone();
                                    if let Some(args) = &app.args {
                                        full_cmd_str.push_str(" ");
                                        full_cmd_str.push_str(&args.join(" "));
                                    }
                                    let current_desc = app.description.clone().unwrap_or_default();
                                    
                                    modal_state = ModalState::Form;
                                    active_form = Some(FormState {
                                        title: "Edit Application",
                                        fields: vec![
                                            FormField { label: "Name", value: app.name.clone(), cursor_pos: app.name.len() },
                                            FormField { label: "Hotkey", value: app.key.clone(), cursor_pos: app.key.len() },
                                            FormField { label: "Command", value: full_cmd_str.clone(), cursor_pos: full_cmd_str.len() },
                                            FormField { label: "Description", value: current_desc.clone(), cursor_pos: current_desc.len() },
                                        ],
                                        active_field: 0,
                                        error_message: None,
                                        is_edit: true,
                                    });
                                }
                            }
                            (KeyCode::Up, _) => {
                                if selected > 0 {
                                    selected -= 1;
                                }
                            }
                            (KeyCode::Down, _) => {
                                if !filtered_apps.is_empty() {
                                    if selected + 1 < filtered_apps.len() {
                                        selected += 1;
                                    }
                                }
                            }
                            (KeyCode::Enter, _) => {
                                if !filtered_apps.is_empty() {
                                    let app = filtered_apps[selected];
                                    launch_app(app)?;
                                }
                            }
                            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                                if let Some(app) = config.apps.iter().find(|a| a.key == c.to_string()) {
                                    launch_app(app)?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
