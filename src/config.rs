use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct App {
    pub name: String,
    pub cmd: String,
    pub key: String,
    // Optional explicit arguments to avoid shell parsing
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub apps: Vec<App>,
}

impl Config {
    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let toml_string = toml::to_string(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(path, toml_string)
    }
}
