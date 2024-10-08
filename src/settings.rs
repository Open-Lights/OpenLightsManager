use chrono::serde::ts_seconds_option;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;
use chrono::{DateTime, Duration, Local, Utc};
use chrono::format::StrftimeItems;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub unstable_releases: bool,
    pub dark_theme: bool,
    pub jvm_path: String,
    pub github_token: String,
    #[serde(with = "ts_seconds_option")]
    pub last_github_check: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub last_github_check_formatted: String,
    pub override_rate_limit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            unstable_releases: false,
            dark_theme: true,
            jvm_path: String::new(),
            github_token: String::new(),
            last_github_check: Some(Utc::now() - Duration::hours(1)),
            override_rate_limit: false,
            last_github_check_formatted: (Utc::now() - Duration::hours(1)).format("%H:%M:%S - %m/%d/%Y").to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MigrationSettings {
    pub unstable_releases: Option<bool>,
    pub dark_theme: Option<bool>,
    pub jvm_path: Option<String>,
    pub github_token: Option<String>,
    #[serde(with = "ts_seconds_option")]
    pub last_github_check: Option<DateTime<Utc>>,
    pub override_rate_limit: Option<bool>,
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
        self.save_formatted_time();
    }

    fn save_formatted_time(&mut self) {
        let local_last_github_check = self.last_github_check.unwrap().with_timezone(&Local::now().timezone());
        let formatted_time = local_last_github_check.format_with_items(StrftimeItems::new("%I:%M:%S %p - %m/%d/%Y")).to_string();
        self.last_github_check_formatted = formatted_time;
    }
}

fn fix_settings(buf_reader: BufReader<File>) -> Settings {
    let mut settings = Settings::default();
    let incomplete_json: Result<MigrationSettings, serde_json::Error> = serde_json::from_reader(buf_reader);
    if let Ok(scavenged_json) = incomplete_json {
        settings.unstable_releases = scavenged_json.unstable_releases.unwrap_or(settings.unstable_releases);
        settings.dark_theme = scavenged_json.dark_theme.unwrap_or(settings.dark_theme);
        settings.jvm_path = scavenged_json.jvm_path.unwrap_or(settings.jvm_path.clone());
        settings.github_token = scavenged_json.github_token.unwrap_or(settings.github_token.clone());
        settings.last_github_check = if let Some(gh_check) = scavenged_json.last_github_check {
            Some(gh_check)
        } else {
            Some(Utc::now() - Duration::hours(1))
        };
        settings.override_rate_limit = scavenged_json.override_rate_limit.unwrap_or(settings.override_rate_limit);
    }
    settings.save_settings();
    settings
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
        let mut reader = BufReader::new(file);
        let result: Result<Settings, serde_json::Error> = serde_json::from_reader(&mut reader);
        if let Ok(mut json) = result {
            json.save_formatted_time();
            json
        } else {
            fix_settings(reader)
        }
    }
}

fn create_settings(path: &Path) -> File {
    fs::create_dir_all("openlightsmanager/").unwrap();
    File::create(path).unwrap()
}