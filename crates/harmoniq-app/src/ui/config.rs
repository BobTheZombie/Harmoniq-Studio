use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::commands::CommandId;

const APP_DIR_NAME: &str = "HarmoniqStudio";
const SHORTCUTS_FILE: &str = "shortcuts.json";
const RECENT_FILE: &str = "recent.json";

pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("config directory unavailable")?;
    let dir = base.join(APP_DIR_NAME);
    if !dir.exists() {
        fs::create_dir_all(&dir).context("create config directory")?;
    }
    Ok(dir)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShortcutEntry {
    pub command: CommandId,
    pub binding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShortcutFile {
    pub bindings: Vec<ShortcutEntry>,
}

pub fn load_shortcut_file() -> ShortcutFile {
    let path = match config_dir() {
        Ok(dir) => dir.join(SHORTCUTS_FILE),
        Err(_) => return ShortcutFile::default(),
    };
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => ShortcutFile::default(),
    }
}

pub fn save_shortcut_file(file: &ShortcutFile) -> Result<()> {
    let dir = config_dir()?;
    let path = dir.join(SHORTCUTS_FILE);
    let json = serde_json::to_string_pretty(file)?;
    let mut f = fs::File::create(path)?;
    f.write_all(json.as_bytes())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecentProjectsFile {
    pub entries: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct RecentProjects {
    entries: Vec<PathBuf>,
}

impl RecentProjects {
    pub fn load() -> Self {
        let path = match config_dir() {
            Ok(dir) => dir.join(RECENT_FILE),
            Err(_) => return Self::default(),
        };
        match fs::read_to_string(&path) {
            Ok(data) => match serde_json::from_str::<RecentProjectsFile>(&data) {
                Ok(file) => Self {
                    entries: file.entries,
                },
                Err(_) => Self::default(),
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir()?;
        let file = RecentProjectsFile {
            entries: self.entries.clone(),
        };
        let json = serde_json::to_string_pretty(&file)?;
        let mut handle = fs::File::create(dir.join(RECENT_FILE))?;
        handle.write_all(json.as_bytes())?;
        Ok(())
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    pub fn add<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref().to_path_buf();
        self.entries.retain(|entry| entry != &path);
        self.entries.insert(0, path);
        if self.entries.len() > 10 {
            self.entries.truncate(10);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RecentProjects;
    use std::path::PathBuf;

    #[test]
    fn recent_projects_dedup_and_order() {
        let mut recents = RecentProjects::default();
        let first = PathBuf::from("/tmp/project1.hsq");
        let second = PathBuf::from("/tmp/project2.hsq");
        recents.add(&first);
        recents.add(&second);
        recents.add(&first);
        let entries = recents.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], first);
        assert_eq!(entries[1], second);
    }

    #[test]
    fn recent_projects_caps_at_ten() {
        let mut recents = RecentProjects::default();
        for idx in 0..12 {
            recents.add(PathBuf::from(format!("/tmp/project{idx}.hsq")));
        }
        assert_eq!(recents.entries().len(), 10);
        assert_eq!(recents.entries()[0], PathBuf::from("/tmp/project11.hsq"));
        assert_eq!(recents.entries()[9], PathBuf::from("/tmp/project2.hsq"));
    }

    #[test]
    fn recent_projects_moves_existing_to_front() {
        let mut recents = RecentProjects::default();
        let first = PathBuf::from("/tmp/project1.hsq");
        let second = PathBuf::from("/tmp/project2.hsq");
        let third = PathBuf::from("/tmp/project3.hsq");
        recents.add(&first);
        recents.add(&second);
        recents.add(&third);
        recents.add(&second);
        let entries = recents.entries();
        assert_eq!(entries[0], second);
        assert_eq!(entries[1], third);
        assert_eq!(entries[2], first);
    }
}
