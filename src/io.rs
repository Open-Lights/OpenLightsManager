use std::{fs, io, thread};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI8, Ordering};
use std::sync::mpsc::Sender;
use chrono::Utc;
use futures_util::StreamExt;
use reqwest::header;
use semver::{Version, VersionReq};
use serde::Deserialize;
use tokio::runtime::Runtime;
use zip::ZipArchive;
use crate::gui::{App, GithubData, Notification};
use crate::notifications::rate_limit_notification;
use crate::settings::Settings;

const GITHUB_REPOS: [&str; 4] = ["Open-Lights/OpenLightsCore", "Open-Lights/OpenLightsManager", "Open-Lights/BeatMaker", "graalvm/graalvm-ce-builds"];

pub fn gather_app_data(prerelease: bool, settings: &mut Settings) -> (Vec<App>, Option<Notification>) {
    let mut vector = Vec::new();
    let mut checked_github = false;
    for project_str in GITHUB_REPOS {
        let project = String::from(project_str);
        let parts: Vec<&str> = project.split('/').collect();
        let project_name = parts.get(1).unwrap_or(&"");
        let format = format!("openlightsmanager/appdata/{}.json", project_name);
        let path = Path::new(format.as_str());
        println!("Path: {}", format);
        if path.exists() {
            let file = OpenOptions::new()
                .read(true)
                .open(path)
                .unwrap();
            let reader = BufReader::new(file);
            let mut app: App = serde_json::from_reader(reader).unwrap();
            check_for_updates(&mut app, prerelease, settings, false);
            vector.push(app);
        } else {
            fs::create_dir_all("openlightsmanager/appdata/").unwrap();
            if should_check_github(&settings) {
                checked_github = true;
                let latest_data;
                let latest_data_pre = get_latest_version_data(&project, true, false);
                // Rate limited
                if let Some(notification) = latest_data_pre.2 {
                    println!("Github has rate limited us!");
                    return (vector, Some(notification));
                }

                if let Some(release_data) = latest_data_pre.1 {
                    latest_data = (latest_data_pre.0, release_data);
                } else {
                    println!("No valid release or prerelease was found for {}\n", project_name);
                    continue;
                }
                let version = parse_semver(&latest_data.1.tag_name);
                let mut app = App {
                    installed: false,
                    event: InstallationEvents::default(),
                    path: path.to_string_lossy().to_string(),
                    app_path: String::new(),
                    name: project_name.to_string(),
                    version: version.to_string(),
                    image_url: format!("../assets/{}.png", project_name),
                    github_repo: project,
                    github_data: latest_data.0,
                    release_data: latest_data.1,
                    has_update: false,
                    launchable: false,
                };
                let installation_data = get_installation_data(&app);
                app.app_path = installation_data.app_path; //TODO Proper app path
                app.launchable = installation_data.launchable;
                save_app_data(app.clone(), prerelease, settings);
                vector.push(app);
            }
        }
    }
    if checked_github {
        set_checked_for_update(settings);
    }
    (vector, None)
}

// Strict means it must be stable if prefer_stable is true
// If not strict and no stable builds are found, the latest unstable build is provided
pub fn get_latest_version_data(project: &String, prefer_stable: bool, strict: bool) -> (GithubData, Option<crate::gui::ReleaseData>, Option<Notification>) {
    let url = format!("https://api.github.com/repos/{}", project);
    println!("{}", &url);
    let rt = Runtime::new().unwrap();
    let response = rt.block_on(get_json(&url.to_owned()));

    if let Ok(github_data) = serde_json::from_str::<GithubData>(&response) {
        let release_repo_url = &github_data.releases_url;
        let modified_repo_url = release_repo_url.replace("{/id}", "");
        let response_release = rt.block_on(get_json(&modified_repo_url));
        println!("{}", &modified_repo_url);
        let release_response: Result<Vec<crate::gui::ReleaseData>, serde_json::Error> = serde_json::from_str(&response_release);
        if let Ok(release_data) = release_response {
            if !release_data.is_empty() {
                if prefer_stable {
                    for release in release_data.clone() {
                        if !release.prerelease {
                            return (github_data, Some(release), None);
                        }
                    }

                    if !strict {
                        return (github_data, Some(release_data[0].clone()), None);
                    }
                } else {
                    return (github_data, Some(release_data[0].clone()), None);
                }
            }
        }
        return (github_data, None, None); // No releases are present
    }
    // Rate Limited output
    rate_limited_output()
}

pub fn get_version_data(project: String, id: i32) -> (GithubData, Option<crate::gui::ReleaseData>, Option<Notification>) {
    let url = format!("https://api.github.com/repos/{}", project);
    let rt = Runtime::new().unwrap();
    let response = rt.block_on(get_json(&url.to_owned()));

    if let Ok(github_data) = serde_json::from_str::<GithubData>(&response) {
        let release_repo_url = &github_data.releases_url;
        let modified_repo_url = release_repo_url.replace("{/id}", format!("/{}", id).as_str());
        let response_release = rt.block_on(get_json(&modified_repo_url.to_owned()));
        let release_data: crate::gui::ReleaseData = serde_json::from_str(&response_release).unwrap();
        (github_data, Some(release_data), None)
    } else {
        // Rate limited
        rate_limited_output()
    }
}

pub fn should_check_github(settings: &Settings) -> bool {
    if !settings.override_rate_limit {
        let current_time = Utc::now();
        let last_check = settings.last_github_check.unwrap();
        let time_diff = current_time.signed_duration_since(last_check);
        let min_per_check = minutes_between_gh_checks(!settings.github_token.is_empty());
        let required_time_diff = chrono::Duration::minutes(min_per_check);
        println!("Waited time: {}; Required time: {}", time_diff.num_minutes(), required_time_diff.num_minutes());
        time_diff > required_time_diff
    } else {
        true
    }
}

fn minutes_between_gh_checks(authed: bool) -> i64 {
    let minutes_in_hour = 60;
    let requests_per_hour = if authed { 5000 } else { 60 };
    let requests_per_check = GITHUB_REPOS.len() as i32 * 2;
    let unrounded = (minutes_in_hour / requests_per_hour) as f32 * requests_per_check as f32;
    unrounded.ceil() as i64
}

fn rate_limited_output() -> (GithubData, Option<crate::gui::ReleaseData>, Option<Notification>) {
    // Rate Limited output
    let fake_github_data = GithubData {
        description: "Rate limited".to_string(),
        releases_url: "Unknown".to_string(),
    };
    let notification = rate_limit_notification();
    (fake_github_data, None, Some(notification))
}

async fn get_json(url: &String) -> String {
    let client = reqwest::Client::new();
    let resp = client.get(url)
        .header(header::USER_AGENT, "Open-Lights-Manager")
        .send()
        .await
        .expect("Failed to request json")
        .text()
        .await;
    resp.unwrap()
}

pub fn save_apps_data(apps: Vec<App>, prerelease: bool, settings: &mut Settings) {
    for app in apps {
        save_app_data(app, prerelease, settings);
    }
}

pub fn save_app_data(mut app: App, prerelease: bool, settings: &mut Settings) {
    check_for_updates(&mut app, prerelease, settings, false);
    save_app_data_offline(&app);
}

pub fn save_app_data_offline(app: &App) {
    let path: &Path = Path::new(&app.path);
    let file: File = if path.exists() {
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)
            .unwrap()
    } else {
        fs::create_dir_all("openlightsmanager/appdata/").unwrap();
        File::create(path).unwrap();
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)
            .unwrap()
    };
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &app).unwrap();
}

pub fn check_for_all_updates(apps: &mut Vec<App>, prerelease: bool, settings: &mut Settings) {
    for app in apps.iter_mut() {
        check_for_updates(app, prerelease, settings, true);
    }
}

// Override check avoids setting a new time
pub fn check_for_updates(app: &mut App, prerelease: bool, settings: &mut Settings, override_check: bool) {
    if should_check_github(settings) {
        let current_ver = &mut app.version;
        let latest_data = {
            let data = get_latest_version_data(&app.github_repo, !prerelease, true);
            if let Some(release_data) = data.1 {
                (data.0, release_data)
            } else {
                return; // No releases available
            }
        };
        let latest_ver = parse_semver(&latest_data.1.tag_name);
        if is_outdated(parse_semver(current_ver), latest_ver) {
            app.has_update = true;
        }
        if !override_check {
            set_checked_for_update(settings);
        }
    }
}

// Attempts to make it more readable
pub fn clean_github_tag(tag: &String) -> String {
    // Remove everything before the first number
    let first_number_index = tag.chars().position(|c| c.is_numeric()).unwrap_or(tag.len());
    tag[first_number_index..].to_string()
}

pub fn parse_semver(version_str: &String) -> Version {
    let clean_str = clean_github_tag(version_str);
    let version = Version::parse(&clean_str);
    if let Ok(semver) = version {
        semver
    } else {
        // We don't really know the version
        Version {
            major: 0,
            minor: 0,
            patch: 0,
            pre: Default::default(),
            build: Default::default(),
        }
    }
}

// Returns the newer version
pub fn compare_semver(version_1: Version, version_2: Version) -> Version {
    let version_1_str = version_1.to_string();
    let req = VersionReq::parse(format!(">{}", &version_1_str).as_str()).unwrap();
    if req.matches(&version_2) {
        version_2
    } else {
        version_1
    }
}

pub fn is_stable(version: Version) -> bool {
    version.pre.is_empty()
}

pub fn is_outdated(current_ver: Version, other_ver: Version) -> bool {
    let version_1_str = current_ver.to_string();
    let req = VersionReq::parse(format!(">{}", &version_1_str).as_str()).unwrap();
    req.matches(&other_ver)
}

fn set_checked_for_update(settings: &mut Settings) {
    settings.last_github_check = Some(Utc::now());
    settings.save_settings();
}



// File downloading
// Extension includes the period
pub fn download_application(app: &mut App, progress: &Arc<AtomicI8>, sender: Sender<(InstallationEvents, Option<String>)>) {
    let app_clone = app.clone();
    let progress_clone = Arc::clone(&progress);
    thread::spawn(move || {
        let application_path = Path::new("openlightsmanager/apps/");
        if !application_path.exists() {
            fs::create_dir_all(application_path).unwrap();
        }
        for asset in &app_clone.release_data.assets {
            let filename = asset.browser_download_url.split('/').last().unwrap_or("unknown");
            println!("Examining {}", filename);
            let parts: Vec<&str> = filename.split('.').collect();
            let asset_extension = parts.last().unwrap_or(&"").to_string();

            let installation_data = get_installation_data(&app_clone);

            if let Some(extension_comparing) = installation_data.extension {
                if asset_extension.ne(&extension_comparing) {
                    println!("Bad: extension mismatch; Provided {}, Expected {}", asset_extension, extension_comparing);
                    continue;
                }
            }

            if let Some(key) = installation_data.key_word {
                if !filename.contains(key.as_str()) {
                    println!("Bad: key word mismatch");
                    continue;
                }
            }

            println!("Success");
            let path_str = if is_archive(&asset_extension) {
                format!("openlightsmanager/apps/{}", filename)
            } else {
                let parent_str =   format!("openlightsmanager/apps/{}/", &app_clone.name);
                let parent_path = Path::new(&parent_str);
                if !parent_path.exists() {
                    fs::create_dir_all(parent_path).unwrap();
                }
                format!("{}{}", parent_str, filename)
            };
            let rt = Runtime::new().unwrap();
            rt.block_on(get_file(&asset.browser_download_url, path_str.clone(), &progress_clone));

            if is_archive(&asset_extension) {
                send_event(&sender, InstallationEvents::Extracting, None);
                progress_clone.store(0, Ordering::Relaxed);

                let extracted_path_str = if installation_data.has_extra_folder {
                    String::from("openlightsmanager/apps/")
                } else {
                    format!("openlightsmanager/apps/{}/", &app_clone.name)
                };
                let extracted_path = Path::new(&extracted_path_str);
                if !extracted_path.exists() {
                    fs::create_dir(extracted_path).unwrap();
                }
                let path = Path::new(&path_str);
                let file = File::open(path).unwrap();
                let mut archive = ZipArchive::new(file).unwrap();

                let total_files = archive.len();
                for i in 0..total_files {
                    let mut file = archive.by_index(i).unwrap();
                    #[allow(deprecated)]
                    let file_name = file.sanitized_name();
                    let extracted_file_path = extracted_path.join(file_name);

                    if file.is_dir() {
                        fs::create_dir_all(&extracted_file_path).unwrap();
                    } else {
                        let parent = extracted_file_path.parent().unwrap();
                        if !parent.exists() {
                            fs::create_dir_all(parent).unwrap();
                        }
                        let mut extracted_file = File::create(&extracted_file_path).unwrap();
                        io::copy(&mut file, &mut extracted_file).unwrap();
                    }

                    let progress = ((i as f32 + 1.) * 100.) / total_files as f32;
                    let progress_rounded = progress.ceil() as i8;
                    progress_clone.store(progress_rounded, Ordering::Relaxed);
                }

                // App-specific tasks

                if installation_data.has_extra_folder {
                    for entry in extracted_path.read_dir().expect("Failed to read directory") {
                        if let Ok(entry) = entry {
                            let entry_path = entry.path();
                            if entry_path.is_dir() {
                                let new_entry_path_str = format!("openlightsmanager/apps/{}/", &app_clone.name);
                                let new_entry_path = Path::new(&new_entry_path_str);
                                fs::rename(entry_path, new_entry_path).unwrap();
                            }
                        }
                    }
                }

                fs::remove_file(path).unwrap();
            }

            // Is Java
            if installation_data.is_library && filename.contains("jdk") {
                send_event(&sender, InstallationEvents::JavaInstalled, Some(installation_data.app_path));
            } else if installation_data.is_manager {
                // TODO Remove old exe
                send_event(&sender, InstallationEvents::ManagerInstalled, None);
            } else {
                send_event(&sender, InstallationEvents::AppInstalled, None);
            }
            progress_clone.store(0, Ordering::Relaxed);
            println!("Finished Installing!");
            return;
        }
        println!("Failed to install");
        send_event(&sender, InstallationEvents::Failed, None); // TODO Send failure message
        progress_clone.store(0, Ordering::Relaxed);
    });
}

async fn get_file(url: &String, path: String, progress: &Arc<AtomicI8>) {
    let response = reqwest::get(url).await.unwrap();
    let content_length = response.content_length().unwrap_or(0);

    let mut total_bytes_read = 0;
    let mut file = File::create(&path).unwrap();

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        total_bytes_read += chunk.len() as u64;
        file.write_all(&chunk).unwrap();

        let progress_percentage = ((total_bytes_read * 100) as f64 / content_length as f64).round() as i8;
        progress.store(progress_percentage, Ordering::Relaxed);
    }
}

fn is_archive(extension: &String) -> bool {
    match extension.as_str() {
        "zip" => true,
        "rar" => true,
        "7z" => true,
        "tar" => true,
        "gz" => true,
        _ => false,
    }
}

#[derive(Deserialize)]
struct InstallationData {
    launchable: bool,
    is_library: bool,
    is_manager: bool,
    has_extra_folder: bool,
    extension: Option<String>,
    key_word: Option<String>,
    app_path: String,
}

fn get_installation_data(app: &App) -> InstallationData {
    let path_str = format!("assets/{}.json", app.name);
    let path = Path::new(&path_str);
    println!("InstallDataPath: {}", path_str);
    let file = File::open(path).unwrap();
    let mut reader = BufReader::new(file);
    let result: Result<InstallationData, serde_json::Error> = serde_json::from_reader(&mut reader);
    result.unwrap()
}

#[derive(Default, Clone, Debug, PartialEq)]
pub enum InstallationEvents {
    #[default]
    None,
    Downloading,
    Extracting,
    AppInstalled,
    Failed,
    JavaInstalled,
    ManagerInstalled,
}

fn send_event(sender: &Sender<(InstallationEvents, Option<String>)>, event: InstallationEvents, data: Option<String>) {
    sender.send((event, data)).unwrap();
}