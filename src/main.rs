mod config;

use config::Config;
use std::env;
use std::fs;
use std::io::{Write, stdout};
use std::path::PathBuf;
use std::process::Command;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType},
};

fn main() {
    // Load config
    let home = env::var("HOME").expect("No HOME env var found");
    let config_path = PathBuf::from(home).join(".config/term-launcher/config.toml");
    let config_contents = fs::read_to_string(config_path).expect("Failed to read config");
    let config: Config = toml::from_str(&config_contents).expect("Failed to parse config");

    // TUI setup
    let mut stdout = stdout();
    terminal::enable_raw_mode().unwrap();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide).unwrap();

    let mut selected = 0;

    loop {
        // Clear screen and render menu
        execute!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();
        writeln!(stdout, "Term Launcher (↑ ↓ Enter to launch, q to quit)\n").unwrap();

        for (i, app) in config.apps.iter().enumerate() {
            let y = (i + 2) as u16; // Offset to avoid header
            execute!(
                stdout,
                cursor::MoveTo(0, y),
                terminal::Clear(ClearType::CurrentLine)
            )
            .unwrap();

            if i == selected {
                write!(stdout, "> {} ({})\n", app.name, app.key).unwrap();
            } else {
                write!(stdout, "  {} ({})\n", app.name, app.key).unwrap();
            }
        }

        stdout.flush().unwrap();

        // Handle key events
        if let Event::Key(key_event) = event::read().unwrap() {
            match key_event.code {
                KeyCode::Char('q') => break,
                KeyCode::Up => {
                    if selected > 0 {
                        selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if selected < config.apps.len() - 1 {
                        selected += 1;
                    }
                }
                KeyCode::Enter => {
                    let app = &config.apps[selected];

                    // Leave raw mode and screen for launching
                    terminal::disable_raw_mode().unwrap();
                    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show).unwrap();

                    // Optional: Clear screen for app launch
                    Command::new("clear").status().ok();

                    // Launch the app
                    let status = Command::new(&app.cmd)
                        .status()
                        .expect("Failed to launch command");

                    println!("\nProcess exited with status: {}\n", status);
                    println!("Press any key to return to the launcher...");
                    event::read().unwrap();

                    // Re-enter raw mode and UI
                    terminal::enable_raw_mode().unwrap();
                    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide).unwrap();
                }
                _ => {}
            }
        }
    }

    // Final cleanup
    terminal::disable_raw_mode().unwrap();
    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show).unwrap();
}
