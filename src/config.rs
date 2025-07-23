use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct App {
    pub name: String,
    pub cmd: String,
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub apps: Vec<App>,
}
