mod config;

use config::Config;
use std::env;
use std::fs;
use std::io::{stdout, Write};
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

    //Tui setup
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
        println!("Term Launcher (↑ ↓ Enter to launch, q to quit)\n");

        for (i, app) in config.apps.iter().enumerate() {
            if i == selected {
                println!("> {} ({})", app.name, app.key);
            } else {
                println!("  {} ({})", app.name, app.key);
            }
        }

        // Wait for key event
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
                    terminal::disable_raw_mode().unwrap();
                    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show).unwrap();
                    Command::new(&app.cmd)
                        .spawn()
                        .expect("Failed to launch command")
                        .wait()
                        .expect("App crashed?");
                    return;
                }
                _ => {}
            }
        }
    }

    // Clean up
    terminal::disable_raw_mode().unwrap();
    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show).unwrap();
}
