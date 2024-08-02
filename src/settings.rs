use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub unstable_releases: bool,
    pub dark_theme: bool,
    pub jvm_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            unstable_releases: false,
            dark_theme: true,
            jvm_path: String::new(),
        }
    }
}

impl Settings {
    pub fn save_settings(&mut self) {
        let path: &Path = Path::new("openlightsmanager/config.json");
        let file: File = if path.exists() {
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .create(true)
                .open(path)
                .unwrap()
        } else {
            create_settings(path)
        };
        let writer = BufWriter::new(file);
        println!("Saving Settings");
        serde_json::to_writer_pretty(writer, &self).unwrap();
    }
}

pub fn load_settings() -> Settings {
    let path: &Path = Path::new("openlightsmanager/config.json");
    if !path.exists() {
        create_settings(path);
        let mut settings = Settings::default();
        settings.save_settings();
        settings
    } else {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).unwrap()
    }
}

fn create_settings(path: &Path) -> File {
    fs::create_dir_all("openlightsmanager/").unwrap();
    File::create(path).unwrap()
}