mod config;

use std::env;
use std::fs;
use std::path::PathBuf;

use config::Config;

fn main() {
    let home_dir = env::var("HOME").expect("Could not find HOME directory");
    let config_path = PathBuf::from(home_dir).join(".config/term-launcher/config.toml");

    let config_contents = fs::read_to_string(&config_path).expect("Failed to read config file");

    let config: Config = toml::from_str(&config_contents).expect("Failed to parse config file");

    println!("Loaded config:");
    for app in config.apps {
        println!("→ {} ({}) - launches `{}`", app.name, app.key, app.cmd);
    }
}
