use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::Path;
use tokio::runtime::Runtime;
use crate::gui::{App, Version};

const GITHUB_REPOS: [&str; 3] = ["Open-Lights/OpenLightsCore", "Open-Lights/OpenLightsManager", "Open-Lights/BeatMaker"];

pub fn gather_app_data(prerelease: bool) -> Vec<App> {
    let mut vector = Vec::new();
    for project_str in GITHUB_REPOS {
        let project = String::from(project_str);
        let parts: Vec<&str> = project.split('/').collect();
        let project_name = parts.get(1).unwrap_or(&"");
        let format = format!("openlightsmanager/appdata/{}.json", project_name);
        let path = Path::new(format.as_str());
        if path.exists() {
            let file = OpenOptions::new()
                .read(true)
                .create(true)
                .open(path)
                .unwrap();
            let reader = BufReader::new(file);
            let mut app: App = serde_json::from_reader(reader).unwrap();
            check_for_updates(&mut app, prerelease);
            vector.push(app);
        } else {
            fs::create_dir_all("openlightsmanager/appdata/").unwrap();
            File::create(path).unwrap();
            let latest_data;
            let latest_data_pre = get_latest_stable_version_data(&project);
            if let Some(release_data) = latest_data_pre.1 {
                latest_data = (latest_data_pre.0, release_data);
            } else {
                latest_data = get_latest_version_data(&project);
            }
            let app = App {
                installed: false,
                path: path.to_string_lossy().to_string(),
                name: project_name.to_string(),
                version: Version::from_string(latest_data.1.tag_name.clone(), latest_data.1.prerelease),
                image_url: format!("../assets/{}.png", project_name),
                github_repo: project,
                github_data: latest_data.0,
                release_data: latest_data.1,
                has_update: false,
                launchable: false, //TODO Determine if it is launchable during installation
            };
            save_app_data(app.clone(), prerelease);
            vector.push(app);
        }
    }
    vector
}

pub fn get_latest_version_data(project: &String) -> (crate::gui::GithubData, crate::gui::ReleaseData) {
    let url = format!("https://api.github.com/repos/{}", project);
    let rt = Runtime::new().unwrap();
    let response = rt.block_on(get_json(&url.to_owned()));
    let github_data: crate::gui::GithubData = serde_json::from_str(&response).unwrap();
    let release_repo_url = &github_data.releases_url;
    let modified_repo_url = release_repo_url.replace("{/id}", format!("/{}", "").as_str());
    let response_release = rt.block_on(get_json(&modified_repo_url.to_owned()));
    let release_data: Vec<crate::gui::ReleaseData> = serde_json::from_str(&response_release).unwrap();
    (github_data, release_data[0].clone())
}

pub fn get_latest_stable_version_data(project: &String) -> (crate::gui::GithubData, Option<crate::gui::ReleaseData>) {
    let url = format!("https://api.github.com/repos/{}", project);
    let rt = Runtime::new().unwrap();
    let response = rt.block_on(get_json(&url.to_owned()));
    let github_data: crate::gui::GithubData = serde_json::from_str(&response).unwrap();
    let release_repo_url = &github_data.releases_url;
    let modified_repo_url = release_repo_url.replace("{/id}", format!("/{}", "").as_str());
    let response_release = rt.block_on(get_json(&modified_repo_url.to_owned()));
    let release_data: Vec<crate::gui::ReleaseData> = serde_json::from_str(&response_release).unwrap();
    for release in release_data {
        if !release.prerelease {
            return (github_data, Some(release));
        }
    }
    (github_data, None)
}

pub fn get_version_data(project: String, id: i32) -> (crate::gui::GithubData, crate::gui::ReleaseData) {
    let url = format!("https://api.github.com/repos/{}", project);
    let rt = Runtime::new().unwrap();
    let response = rt.block_on(get_json(&url.to_owned()));
    let github_data: crate::gui::GithubData = serde_json::from_str(&response).unwrap();
    let release_repo_url = &github_data.releases_url;
    let modified_repo_url = release_repo_url.replace("{/id}", format!("/{}", id).as_str());
    let response_release = rt.block_on(get_json(&modified_repo_url.to_owned()));
    let release_data: crate::gui::ReleaseData = serde_json::from_str(&response_release).unwrap();
    (github_data, release_data)
}

async fn get_json(url: &String) -> String {
    let resp = reqwest::get(url).await.unwrap().text().await;
    resp.unwrap()
}

pub fn save_apps_data(apps: Vec<App>, prerelease: bool) {
    for app in apps {
        save_app_data(app, prerelease);
    }
}

pub fn save_app_data(mut app: App, prerelease: bool) {
    check_for_updates(&mut app, prerelease);
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
        File::create(path).unwrap()
    };
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &app).unwrap();
}

pub fn check_for_updates(app: &mut App, prerelease: bool) {
    let current_ver = &mut app.version;
    let latest_data = if prerelease {
        get_latest_version_data(&app.github_repo)
    } else {
        let data = get_latest_stable_version_data(&app.github_repo);
        if let Some(release_data) = data.1 {
            (data.0, release_data)
        } else {
            return; // No stable versions at all
        }
    };
    let latest_ver = Version::from_string(latest_data.1.tag_name, latest_data.1.prerelease);
    if current_ver.is_old(latest_ver) {
        app.has_update = true;
    }
}