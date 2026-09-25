//! Persisted session state: recents, last range per repo, view toggles.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Persisted {
    #[serde(default)]
    pub recents: Vec<PathBuf>,
    #[serde(default)]
    pub last_repo: Option<PathBuf>,
    /// repo path → (base, compare)
    #[serde(default)]
    pub ranges: HashMap<String, (String, String)>,
    #[serde(default)]
    pub split: bool,
    #[serde(default)]
    pub ignore_ws: bool,
    #[serde(default = "yes")]
    pub tree: bool,
    #[serde(default)]
    pub sidebar_w: Option<f32>,
    #[serde(default)]
    pub range_collapsed: bool,
    #[serde(default)]
    pub banner_collapsed: bool,
    #[serde(default)]
    pub banner_h: Option<f32>,
}

fn yes() -> bool {
    true
}

fn file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/kerf/state.json"))
}

impl Persisted {
    pub fn load() -> Self {
        file()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| Self { tree: true, ..Default::default() })
    }

    pub fn save(&self) {
        let Some(f) = file() else { return };
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(f, json);
        }
    }

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recents.retain(|p| p != &path);
        self.recents.insert(0, path.clone());
        self.recents.truncate(12);
        self.last_repo = Some(path);
    }
}
