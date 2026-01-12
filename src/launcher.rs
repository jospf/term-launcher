use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

pub fn resolve_command(cmd: &str) -> Option<PathBuf> {
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
