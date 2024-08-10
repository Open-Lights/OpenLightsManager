use std::{fs, io, thread};
use std::env::current_dir;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicI8, AtomicU32, Ordering};
use std::sync::mpsc::Sender;

use chrono::Utc;
use futures_util::StreamExt;
use reqwest::header;
use semver::{Version, VersionReq};
use serde::Deserialize;
use tokio::runtime::Runtime;
use zip::ZipArchive;

use crate::gui::{App, GithubData, Notification, ReleaseData, ThreadCommunication};
use crate::notifications::{launched_application, launched_application_missing_java, rate_limit_notification};
use crate::settings::Settings;

const GITHUB_REPOS: [&str; 6] = ["Open-Lights/OpenLightsCore", "Open-Lights/OpenLightsManager", "Open-Lights/BeatMaker", "Open-Lights/Christmas-Jukebox", "Open-Lights/BeatFileEditor", "graalvm/graalvm-ce-builds"];

pub fn gather_app_data(prerelease: bool, settings: &mut Settings) -> (Vec<App>, Option<Notification>) { // TODO Fix an issue with prerelease being true and not loading stable releases
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
            app.installation_data = get_installation_data(&app);
            if app.installation_data.is_manager {
                app.installed = true;
                // TODO Write the first manager json to file
            }
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
                    event: AppEvents::default(),
                    path: path.to_string_lossy().to_string(),
                    app_path: String::new(),
                    name: project_name.to_string(),
                    version: version.to_string(),
                    image_url: format!("../assets/{}.png", project_name),
                    github_repo: project,
                    github_data: latest_data.0,
                    release_data: latest_data.1,
                    has_update: false,
                    update_download_url: None,
                    launchable: false,
                    progress: Arc::new(AtomicI8::new(0)),
                    thread_communication: ThreadCommunication::default(),
                    process: Arc::new(AtomicU32::new(0)),
                    installation_data: InstallationData::default(),
                };
                let installation_data = get_installation_data(&app);
                app.app_path = installation_data.app_path.clone();
                app.launchable = installation_data.launchable;
                app.installation_data = installation_data;
                if app.installation_data.is_manager {
                    app.installed = true;
                    // TODO Write the first manager json to file
                }
                save_app_data_offline(&app);
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
pub fn get_latest_version_data(project: &String, prefer_stable: bool, strict: bool) -> (GithubData, Option<ReleaseData>, Option<Notification>) {
    let url = format!("https://api.github.com/repos/{}", project);
    println!("{}", &url);
    let rt = Runtime::new().unwrap();
    let response = rt.block_on(get_json(&url.to_owned()));

    if let Ok(github_data) = serde_json::from_str::<GithubData>(&response) {
        let release_repo_url = &github_data.releases_url;
        let modified_repo_url = release_repo_url.replace("{/id}", "");
        let response_release = rt.block_on(get_json(&modified_repo_url));
        println!("{}", &modified_repo_url);
        let release_response: Result<Vec<ReleaseData>, serde_json::Error> = serde_json::from_str(&response_release);
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

pub fn get_version_data(project: String, id: i32) -> (GithubData, Option<ReleaseData>, Option<Notification>) {
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

fn rate_limited_output() -> (GithubData, Option<ReleaseData>, Option<Notification>) {
    // Rate Limited output
    let fake_github_data = GithubData {
        description: "Rate limited".to_string(),
        archived: false,
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

pub fn save_apps_data(mut apps: Vec<App>, prerelease: bool, settings: &mut Settings) {
    for mut app in apps.iter_mut() {
        save_app_data(&mut app, prerelease, settings);
    }
}

pub fn save_app_data(app: &mut App, prerelease: bool, settings: &mut Settings) {
    check_for_updates(app, prerelease, settings, false);
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
    if !app.github_data.archived && should_check_github(settings) {
        println!("CHECKING FOR UPDATES");
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
        println!("Current Ver: {}, New Ver: {}", current_ver, latest_ver.to_string());
        if is_outdated(parse_semver(current_ver), latest_ver) {
            app.has_update = true;
            let installation_data = get_installation_data(&app);
            let download_url = locate_asset(&latest_data.1, &installation_data);
            app.update_download_url = Some(download_url);
            app.release_data = latest_data.1;
            save_app_data_offline(&app);
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
    println!("Is outdated (>{}): {}", &version_1_str, req.matches(&other_ver));
    req.matches(&other_ver)
}

fn set_checked_for_update(settings: &mut Settings) {
    settings.last_github_check = Some(Utc::now());
    settings.save_settings();
}



// File downloading
// Extension includes the period
pub fn download_application(app: &App, progress: &Arc<AtomicI8>, sender: &Arc<Sender<(AppEvents, Option<String>)>>) {
    let installation_data = get_installation_data(&app);
    let release_data = app.release_data.clone();
    let name = app.name.clone();
    let progress_clone = Arc::clone(&progress);
    let sender_clone = Arc::clone(sender);
    thread::spawn(move || {
        let application_path = Path::new("openlightsmanager/apps/");
        if !application_path.exists() {
            fs::create_dir_all(application_path).unwrap();
        }
        for asset in &release_data.assets {
            let filename = asset.browser_download_url.split('/').last().unwrap_or("unknown");
            println!("Examining {}", filename);
            let parts: Vec<&str> = filename.split('.').collect();
            let asset_extension = parts.last().unwrap_or(&"").to_string();

            if let Some(extension_comparing) = &installation_data.extension {
                if asset_extension.ne(extension_comparing) {
                    println!("Bad: extension mismatch; Provided {}, Expected {}", asset_extension, extension_comparing);
                    continue;
                }
            }

            if let Some(key) = &installation_data.key_word {
                if !filename.contains(key.as_str()) {
                    println!("Bad: key word mismatch");
                    continue;
                }
            }

            println!("Success");
            let path_str = download(&asset_extension, filename, &name, &progress_clone, &asset.browser_download_url, &installation_data);

            extract(&asset_extension, &sender_clone, &progress_clone, &installation_data, &name, &path_str);

            finalize_download(&installation_data, &sender_clone, filename, &progress_clone);
            return;
        }
        println!("Failed to install");
        send_event(&sender_clone, AppEvents::Failed, None); // TODO Send failure message
        progress_clone.store(0, Ordering::Relaxed);
    });
}

fn download(asset_extension: &String, filename: &str, name: &String, progress_clone: &Arc<AtomicI8>, download_url: &String, installation_data: &InstallationData) -> String {
    let path_str;
    if installation_data.is_manager {
        path_str = format!("{}/{}", current_dir().unwrap().to_string_lossy(), format!("NEW-{}", filename));
    } else {
        path_str = if is_archive(&asset_extension) {
            format!("openlightsmanager/apps/{}", filename)
        } else {
            let parent_str =   format!("openlightsmanager/apps/{}/", name);
            let parent_path = Path::new(&parent_str);
            if !parent_path.exists() {
                fs::create_dir_all(parent_path).unwrap();
            }
            format!("{}{}", parent_str, filename)
        };
    }

    let rt = Runtime::new().unwrap();
    rt.block_on(get_file(download_url, path_str.clone(), &progress_clone));
    path_str
}

fn extract(asset_extension: &String, sender: &Sender<(AppEvents, Option<String>)>, progress_clone: &Arc<AtomicI8>, installation_data: &InstallationData, name: &String, path_str: &String) {
    if is_archive(&asset_extension) {
        send_event(&sender, AppEvents::Extracting, None);
        progress_clone.store(0, Ordering::Relaxed);

        let extracted_path_str = if installation_data.has_extra_folder {
            String::from("openlightsmanager/apps/")
        } else {
            format!("openlightsmanager/apps/{}/", name)
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
                    if entry_path.is_dir() && entry_path.file_name().unwrap().to_string_lossy().contains(&installation_data.extra_folder_key_word.clone().unwrap().as_str()) {
                        let new_entry_path_str = format!("openlightsmanager/apps/{}/", name);
                        let new_entry_path = Path::new(&new_entry_path_str);
                        fs::rename(entry_path, new_entry_path).unwrap();
                    }
                }
            }
        }

        fs::remove_file(path).unwrap();
    }
}

fn finalize_download(installation_data: &InstallationData, sender: &Sender<(AppEvents, Option<String>)>, filename: &str, progress_clone: &Arc<AtomicI8>) {
    // Is Java
    if installation_data.is_library && filename.contains("jdk") {
        send_event(&sender, AppEvents::JavaInstalled, Some(installation_data.app_path.clone()));
    } else if installation_data.is_manager {
        send_event(&sender, AppEvents::ManagerInstalled, None);
    } else {
        send_event(&sender, AppEvents::AppInstalled, None);
    }
    progress_clone.store(0, Ordering::Relaxed);
    println!("Finished Installing!");
}

fn locate_asset(release_data: &ReleaseData, installation_data: &InstallationData) -> String {
    for asset in &release_data.assets {
        let filename = asset.browser_download_url.split('/').last().unwrap_or("unknown");
        let parts: Vec<&str> = filename.split('.').collect();
        let asset_extension = parts.last().unwrap_or(&"").to_string();

        if let Some(extension_comparing) = &installation_data.extension {
            if asset_extension.ne(extension_comparing) {
                continue;
            }
        }

        if let Some(key) = &installation_data.key_word {
            if !filename.contains(key.as_str()) {
                continue;
            }
        }

        return asset.browser_download_url.clone();
    }
    String::new()
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

pub fn update(app: &App, progress: &Arc<AtomicI8>, sender: &Arc<Sender<(AppEvents, Option<String>)>>) {
    let download_url = <Option<String> as Clone>::clone(&app.update_download_url).unwrap();
    let installation_data = get_installation_data(&app);
    let filename = download_url.split('/').last().clone().unwrap_or("unknown").to_string();
    let parts: Vec<&str> = filename.split('.').collect();
    let asset_extension = parts.last().unwrap_or(&"").to_string();
    let name = app.name.clone();
    let progress_clone = Arc::clone(&progress);
    let sender_clone = Arc::clone(sender);
    thread::spawn(move || {
        // Clear old files
        if is_archive(&asset_extension) {
            // None of my apps would come in archive form, so it's safe to delete the entire thing
            let path_str = format!("openlightsmanager/apps/{}", &name);
            let path = Path::new(&path_str);

            if path.exists() {
                fs::remove_dir_all(path).unwrap();
            }
        } else {
            let path_str = format!("openlightsmanager/apps/{}/{}", &name, &filename);
            let path = Path::new(&path_str);

            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }

        // Download new version
        let path_str = download(&asset_extension, &filename, &name, &progress_clone, &download_url, &installation_data);

        extract(&asset_extension, &sender_clone, &progress_clone, &installation_data, &name, &path_str);

        finalize_download(&installation_data, &sender_clone, &filename, &progress_clone);
    });
}

pub fn update_app_data(app: &mut App) {
    app.version = app.release_data.tag_name.clone();
    app.update_download_url = None;
    app.has_update = false;
    save_app_data_offline(&app);
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

#[derive(Deserialize, Debug)]
pub struct InstallationData {
    pub launchable: bool,
    launch_cmd: Option<String>, // Use {jvm} for java path; Use {app} for app path to executable
    pub is_library: bool,
    pub is_manager: bool,
    has_extra_folder: bool,
    extra_folder_key_word: Option<String>,
    extension: Option<String>,
    key_word: Option<String>,
    pub app_path: String,
}

impl Default for InstallationData {
    fn default() -> Self {
        InstallationData {
            launchable: false,
            launch_cmd: None,
            is_library: false,
            is_manager: false,
            has_extra_folder: false,
            extra_folder_key_word: None,
            extension: None,
            key_word: None,
            app_path: String::new(),
        }
    }
}

pub fn get_installation_data(app: &App) -> InstallationData {
    let path_str = format!("assets/{}.json", app.name);
    let path = Path::new(&path_str);
    let file = File::open(path).unwrap();
    let mut reader = BufReader::new(file);
    let result: Result<InstallationData, serde_json::Error> = serde_json::from_reader(&mut reader);
    result.unwrap()
}

pub fn launch_application(app: &mut App, jvm_path_og: &String) -> Notification {
    app.event = AppEvents::Running;
    let installation_data = get_installation_data(&app);
    let app_name = app.name.clone();
    let jvm_path = jvm_path_og.clone();
    let id_clone = Arc::clone(&app.process);
    let cmd = installation_data.launch_cmd.unwrap_or(format!("{}{}", app_name, installation_data.app_path));
    if cmd.contains("{jvm}") && jvm_path.is_empty() {
        return launched_application_missing_java(&app_name);
    }

    thread::spawn(move || {
        let main_argument_path = if cmd.contains("{jvm}") {
            Path::new(&jvm_path)
        } else {
            Path::new(&cmd)
        };
        let app_path = format!("openlightsmanager/apps/{}/", app_name);
        let dir = Path::new(app_path.as_str());
        let filled_in = cmd.replace("{jvm} ", "").replace("{app}", installation_data.app_path.replace("/", "").as_str());
        let split: Vec<&str> = filled_in.split_whitespace().collect();
        id_clone.store(Command::new(main_argument_path)
                           .current_dir(dir)
                           .args(split)
                           .spawn()
                           .expect("Failed to run application")
                           .id(), Ordering::Relaxed);
    });

    launched_application(&app.name)
}

#[derive(Default, Clone, Debug, PartialEq)]
pub enum AppEvents {
    #[default]
    None,
    Downloading,
    Extracting,
    AppInstalled,
    Failed,
    JavaInstalled,
    ManagerInstalled,
    Running,
}

fn send_event(sender: &Sender<(AppEvents, Option<String>)>, event: AppEvents, data: Option<String>) {
    sender.send((event, data)).unwrap();
}