// Copyright (c) 2025 Omair Kamil
// See LICENSE file in root directory for license terms.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const PREFS_FILENAME: &str = "cb_prefs.json";

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct AppPrefs {
    pub brightness: Option<u8>,
}

pub fn get_prefs_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = std::env::current_exe()?;
    path.pop();
    path.push(PREFS_FILENAME);
    Ok(path)
}

pub fn load_brightness() -> u8 {
    if let Ok(path) = get_prefs_path() {
        if let Ok(contents) = std::fs::read_to_string(path) {
            return serde_json::from_str::<AppPrefs>(&contents)
                .ok()
                .and_then(|p| p.brightness)
                .unwrap_or(0x80);
        }
    }
    0x80
}

pub fn save_brightness(brightness: u8) {
    if let Ok(path) = get_prefs_path() {
        let prefs = AppPrefs {
            brightness: Some(brightness),
        };
        if let Ok(data) = serde_json::to_string_pretty(&prefs) {
            let _ = std::fs::write(path, data);
        }
    }
}
