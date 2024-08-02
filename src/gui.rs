use std::cmp::PartialEq;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::i32;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::{Duration, Instant};

use egui::{CentralPanel, Color32, Context, FontFamily, FontId, Frame, Image, Pos2, pos2, Rect, RichText, Rounding, Stroke, TextStyle, Ui, Vec2};
use egui::TextStyle::Body;
use egui_file::FileDialog;
use serde::{Deserialize, Serialize};

use crate::settings::{load_settings, Settings};

pub struct OpenLightsManager {
    current_screen: Screen,
    notifications: VecDeque<Notification>,
    apps: Vec<App>,
    settings: Settings,
    theme: Theme,
    file_explorer: FileExplorer,
}

#[derive(PartialEq, Default)]
enum Screen {
    #[default]
    Installed,
    Settings,
    Browse,
}

#[inline]
fn heading2() -> TextStyle {
    TextStyle::Name("Heading2".into())
}

#[inline]
fn heading3() -> TextStyle {
    TextStyle::Name("ContextHeading".into())
}

#[inline]
fn notification_font() -> TextStyle {
    TextStyle::Name("Notification".into())
}

fn configure_text_styles(ctx: &Context) {
    use FontFamily::Proportional;
    use TextStyle::*;

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (Heading, FontId::new(100.0, Proportional)),
        (heading2(), FontId::new(30.0, Proportional)),
        (heading3(), FontId::new(20.0, Proportional)),
        (notification_font(), FontId::new(12.0, Proportional)),
        (Body, FontId::new(18.0, Proportional)),
        (Monospace, FontId::new(14.0, Proportional)),
        (Button, FontId::new(14.0, Proportional)),
        (Small, FontId::new(10.0, Proportional)),
    ]
        .into();
    ctx.set_style(style);
}

impl eframe::App for OpenLightsManager {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        show_notification(ctx, &mut self.notifications, &self.theme);

        CentralPanel::default().show(ctx, |ui| {
            self.file_explorer.render(ctx, &mut self.settings);
            if let Some(file_explorer) = &self.file_explorer.open_file_dialog {
                ui.set_enabled(!file_explorer.visible());
            }
            update_theme(ui, &self);
            self.render_background(ui);
            self.render_taskbar(ui);

            match self.current_screen {
                Screen::Installed => self.render_installation(ui),
                Screen::Browse => self.render_browse(ui),
                Screen::Settings => self.render_settings(ui),
            }
        });
    }
}

fn update_theme(ui: &mut Ui, open_lights_manager: &&mut OpenLightsManager) {
    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = open_lights_manager.theme.button;
    ui.style_mut().visuals.widgets.inactive.bg_fill = open_lights_manager.theme.button;
    ui.style_mut().visuals.widgets.active.weak_bg_fill = open_lights_manager.theme.clicked;
    ui.style_mut().visuals.widgets.active.bg_fill = open_lights_manager.theme.clicked;
    ui.style_mut().visuals.widgets.hovered.weak_bg_fill = open_lights_manager.theme.hovered;
    ui.style_mut().visuals.widgets.hovered.bg_fill = open_lights_manager.theme.hovered;
}

impl OpenLightsManager {
    pub fn new(ctx: &Context) -> Self {
        configure_text_styles(ctx);

        let settings = load_settings();
        //let apps = gather_app_data(settings.unstable_releases);
        let apps = Vec::new();
        let theme = Theme::get_theme(&settings);
        let file_explorer = FileExplorer {
            opened_file: None,
            open_file_dialog: None,
        };

        OpenLightsManager {
            current_screen: Screen::default(),
            notifications: VecDeque::new(),
            apps,
            settings,
            theme,
            file_explorer,
        }
    }

    pub fn render_background(&mut self, ui: &mut Ui) {
        let rect = Rect::from_two_pos(Pos2 {x: -10., y: 0.}, Pos2 {x: 610., y: 610.});
        Image::new(egui::include_image!("../assets/background.png"))
            .fit_to_exact_size(Vec2 {x: 620., y: 620.})
            .paint_at(ui, rect);
    }

    pub fn render_taskbar(&mut self, ui: &mut Ui) {

        let rect = Rect::from_two_pos(Pos2 {x: 20., y: 80.}, Pos2 {x: 580., y: 130.});
        ui.painter().rect(rect, Rounding::same(16.), self.theme.panel, Stroke::NONE);

        let rect1 = Rect::from_two_pos(pos2(80., 80.), pos2(150., 130.));
        if ui.put(rect1,
                  egui::Label::new(RichText::new("Installed").color(self.theme.text))
        ).clicked() {
            self.current_screen = Screen::Installed;
        };

        let rect2 = Rect::from_two_pos(pos2(250., 80.), pos2(350., 130.));
        if ui.put(rect2,
                  egui::Label::new(RichText::new("Browse").color(self.theme.text))
        ).clicked() {
            self.current_screen = Screen::Browse;
        };

        let rect3 = Rect::from_two_pos(pos2(450., 80.), pos2(520., 130.));
        if ui.put(rect3,
                  egui::Label::new(RichText::new("Settings").color(self.theme.text))
        ).clicked() {
            self.current_screen = Screen::Settings;
        };

        match self.current_screen {
            Screen::Installed => {
                let rect4 = Rect::from_two_pos(pos2(80., 120.), pos2(150., 125.));
                ui.painter().rect(rect4, Rounding::same(16.), self.theme.text, Stroke::NONE);
            }
            Screen::Browse => {
                let rect4 = Rect::from_two_pos(pos2(265., 120.), pos2(335., 125.));
                ui.painter().rect(rect4, Rounding::same(16.), self.theme.text, Stroke::NONE);
            }
            Screen::Settings => {
                let rect4 = Rect::from_two_pos(pos2(450., 120.), pos2(520., 125.));
                ui.painter().rect(rect4, Rounding::same(16.), self.theme.text, Stroke::NONE);
            }
        }
    }

    pub fn render_installation(&mut self, ui: &mut Ui) {
        let rect = Self::tab_area();
        ui.painter().rect(rect, Rounding::same(16.), self.theme.panel, Stroke::NONE);
    }

    pub fn render_browse(&mut self, ui: &mut Ui) {
        let rect = Self::tab_area();
        ui.painter().rect(rect, Rounding::same(16.), self.theme.panel, Stroke::NONE);
    }

    pub fn render_settings(&mut self, ui: &mut Ui) {
        let rect = Self::tab_area();
        ui.painter().rect(rect, Rounding::same(16.), self.theme.panel, Stroke::NONE);
        self.render_settings_panel(ui);
    }

    fn render_app_panel(&mut self, ui: &mut Ui) {
        let rect = Self::scroll_area();

        ui.allocate_ui_at_rect(rect, |ui| {
            egui::ScrollArea::vertical()
                .max_height(420.)
                .max_width(550.)
                .show(ui, |ui| {
                    for (index, app) in self.apps.iter_mut().enumerate() {
                        app.render(ui, index as i8);
                    }
                });
        });
    }

    fn render_settings_panel(&mut self, ui: &mut Ui) {
        let rect = Self::scroll_area();

        ui.allocate_ui_at_rect(rect, |ui| {
            egui::ScrollArea::vertical()
                .max_height(420.)
                .max_width(550.)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized([100., 50.], egui::Label::new(RichText::new("Theme: ").color(self.theme.text)));
                        let style = if self.settings.dark_theme {
                            "Dark"
                        } else {
                            "Light"
                        };

                        if ui.add_sized([50., 30.], egui::Button::new(RichText::new(style).color(self.theme.text))).clicked() {
                            self.settings.dark_theme = !self.settings.dark_theme;
                            if self.settings.dark_theme {
                                self.theme.dark();
                            } else {
                                self.theme.light();
                            }
                            self.settings.save_settings();
                        };
                    });

                    ui.horizontal(|ui| {
                        ui.add_sized([100., 50.], egui::Label::new(RichText::new("Unstable Releases: ").color(self.theme.text)));
                        if ui.add_sized([50., 30.], egui::Checkbox::without_text(&mut self.settings.unstable_releases)).clicked() {
                            self.settings.save_settings();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.add_sized([100., 50.], egui::Label::new(RichText::new("Java Path: ").color(self.theme.text)));
                        ui.add_sized([200., 50.], egui::Label::new(RichText::new(&self.settings.jvm_path).color(self.theme.text).text_style(notification_font())));
                        if ui.add_sized([50., 30.], egui::Button::new(RichText::new("Locate").color(self.theme.text))).clicked() {
                            self.file_explorer.open();
                        }
                        if ui.add_sized([50., 30.], egui::Button::new(RichText::new("Check").color(self.theme.text))).clicked() {
                            let path = Path::new(&self.settings.jvm_path);
                            let filename = path.file_stem().unwrap().to_string_lossy().to_string();

                            if filename != "java" && filename != "javaw" {
                                let notification = Notification {
                                    title: "Java Check Failure".to_string(),
                                    message: "An invalid Java Runtime has been provided.\nEnsure \"javaw\" or \"java\" has been selected.".to_string(),
                                    timer: Timer::new(Duration::from_secs(15)),
                                    id: fastrand::i32(0..i32::MAX),
                                };
                                self.notifications.push_front(notification);
                                println!("Not a valid Java Installation: {}", filename);
                            } else {
                                let command_output = Command::new(path)
                                    .arg("--version")
                                    .output();

                                match command_output {
                                    Ok(output) => {
                                        if output.status.success() {
                                            let stdout_as_string = std::str::from_utf8(&output.stdout)
                                                .unwrap()
                                                .lines()
                                                .nth(1) // Get the second line
                                                .unwrap_or_default();
                                            let notification = Notification {
                                                title: "Java Check Success".to_string(),
                                                message: format!("{} has been checked.", stdout_as_string),
                                                timer: Timer::new(Duration::from_secs(15)),
                                                id: fastrand::i32(0..i32::MAX),
                                            };
                                            self.notifications.push_front(notification);
                                        } else {
                                            let notification = Notification {
                                                title: "Java Check Failure".to_string(),
                                                message: "The provided Java Runtime is either invalid or corrupted.\nTry a different Java Runtime or reinstall the current one.".to_string(),
                                                timer: Timer::new(Duration::from_secs(15)),
                                                id: fastrand::i32(0..i32::MAX),
                                            };
                                            self.notifications.push_front(notification);
                                        }
                                    }
                                    Err(e) => {
                                        let notification = Notification {
                                            title: "Java Check Failure".to_string(),
                                            message: "Failed to run the Java Check.\nPlease try running the Java Check again.\nIf the issue continues, report the issue on Github.".to_string(),
                                            timer: Timer::new(Duration::from_secs(15)),
                                            id: fastrand::i32(0..i32::MAX),
                                        };
                                        self.notifications.push_front(notification);
                                    }
                                }
                            }
                        }
                    });
                });
        });
    }

    fn scroll_area() -> Rect {
        Rect::from_two_pos(pos2(35., 165.), pos2(565., 565.))
    }

    fn tab_area() -> Rect {
        Rect::from_two_pos(pos2(20., 150.), pos2(580., 580.))
    }
}

pub struct FileExplorer {
    opened_file: Option<PathBuf>,
    open_file_dialog: Option<FileDialog>,
}

impl FileExplorer {
    pub fn render(&mut self, ctx: &Context, settings: &mut Settings) {
        CentralPanel::default().show(ctx, |ui| {
            if let Some(dialog) = &mut self.open_file_dialog {
                if dialog.show(ctx).selected() {
                    if let Some(file) = dialog.path() {
                        self.opened_file = Some(file.to_path_buf());
                    }
                }

                match dialog.state() {
                    egui_file::State::Open => {
                        // Dialog is visible.
                    }
                    egui_file::State::Closed => {
                        // Dialog is not visible.
                    }
                    egui_file::State::Cancelled => {
                        // X or Cancel.
                    }
                    egui_file::State::Selected => {
                        if let Some(path) = &self.opened_file {
                            settings.jvm_path = path.to_string_lossy().to_string();
                            settings.save_settings();
                        }
                    }
                }
            }
        });
    }

    pub fn open(&mut self) {
        let filter = Box::new({
            let ext = Some(OsStr::new("exe"));
            move |path: &Path| -> bool { path.extension() == ext }
        });

        let mut dialog = FileDialog::open_file(self.opened_file.clone())
            .show_files_filter(filter)
            .show_new_folder(false)
            .show_rename(false)
            .title("")
            .default_size([400., 400.]);
        dialog.open();
        self.open_file_dialog = Some(dialog);
    }
}

pub struct Theme {
    panel: Color32,
    text: Color32,
    button: Color32,
    clicked: Color32,
    hovered: Color32,
    notification: Color32,
    outline: Color32,
}

impl Theme {
    pub fn dark(&mut self) {
        self.panel = Color32::from(egui::Rgba::from_rgba_premultiplied(0.003, 0.003, 0.003, 0.85));
        self.text = Color32::from(egui::Rgba::from_rgba_premultiplied(0.5, 0.5, 0.5, 1.0));
        self.button = Color32::from(egui::Rgba::from_rgba_premultiplied(0.05, 0.05, 0.05, 1.0));
        self.clicked = Color32::from(egui::Rgba::from_rgba_premultiplied(0.15, 0.15, 0.15, 1.0));
        self.hovered = Color32::from(egui::Rgba::from_rgba_premultiplied(0.1, 0.1, 0.1, 1.0));
        self.notification = Color32::from(egui::Rgba::from_rgba_premultiplied(0.01, 0.01, 0.01, 1.0));
        self.outline = Color32::from(egui::Rgba::from_rgba_premultiplied(0.15, 0.15, 0.15, 1.0));
    }

    pub fn light(&mut self) {
        self.panel = Color32::from(egui::Rgba::from_rgba_premultiplied(0.5, 0.5, 0.5, 0.85));
        self.text = Color32::BLACK;
        self.button = Color32::from(egui::Rgba::from_rgba_premultiplied(0.3, 0.3, 0.3, 1.0));
        self.clicked = Color32::from(egui::Rgba::from_rgba_premultiplied(0.4, 0.4, 0.4, 1.0));
        self.hovered = Color32::from(egui::Rgba::from_rgba_premultiplied(0.35, 0.35, 0.35, 1.0));
        self.notification = Color32::from(egui::Rgba::from_rgba_premultiplied(0.25, 0.25, 0.25, 1.0));
        self.outline = Color32::from(egui::Rgba::from_rgba_premultiplied(0.4, 0.4, 0.4, 1.0));
    }

    pub fn get_theme(settings: &Settings) -> Self {
        let mut theme = Theme {
            panel: Color32::BLACK,
            text: Color32::BLACK,
            button: Color32::BLACK,
            clicked: Color32::BLACK,
            hovered: Color32::BLACK,
            notification: Color32::BLACK,
            outline: Color32::BLACK,
        };
        if settings.dark_theme {
            theme.dark();
        } else {
            theme.light();
        }
        theme
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct App {
    pub installed: bool,
    pub(crate) name: String,
    pub path: String,
    pub version: Version,
    pub(crate) image_url: String,
    pub github_repo: String,
    pub(crate) github_data: GithubData,
    pub(crate) release_data: ReleaseData,
    pub has_update: bool,
    pub(crate) launchable: bool,
}

impl App {
    pub fn default(
        name: String,
        path: String,
        version: Version,
        image_url: String,
        github_repo: String,
        github_data: GithubData,
        release_data: ReleaseData,
        has_update: bool,
        launchable: bool,
    ) -> Self {
        App {
            installed: false,
            name,
            path,
            version,
            image_url,
            github_repo,
            github_data,
            release_data,
            has_update,
            launchable,
        }
    }
}

impl App {
    pub fn render(&mut self, ui: &mut Ui, index: i8) {
        let img_rect = Rect::from_two_pos(pos2(30., (100 * index) as f32 + 160.), pos2(130., (100 * index) as f32 + 260.));
        app_image(&self.name, ui, img_rect);

        let name_rect = Rect::from_two_pos(pos2(140., (100 * index) as f32 + 170.), pos2(460., (100 * index) as f32 + 210.));
        ui.put(name_rect, egui::Label::new(&self.name));

        let button_rect = Rect::from_two_pos(pos2(470., (100 * index) as f32 + 170.), pos2(570., (100 * index) as f32 + 210.));

        if !&self.installed {
            if ui.put(button_rect, egui::Button::new("Install")).clicked() {
                // TODO Install
            }
        }

        if self.has_update {
            if ui.put(button_rect, egui::Button::new("Update")).clicked() {
                // TODO Update
            }
        }

        let description_rect = Rect::from_two_pos(pos2(140., (100 * index) as f32 + 220.), pos2(460., (100 * index) as f32 + 260.));
        ui.put(description_rect, egui::Label::new(RichText::new(&self.github_data.description).text_style(notification_font())));

        if self.installed {
            let ver_rect = Rect::from_two_pos(pos2(470., (100 * index) as f32 + 220.), pos2(570., (100 * index) as f32 + 260.));
            ui.put(ver_rect, egui::Label::new(&self.version.as_string()));
        }
    }
}

fn app_image(name: &String, ui: &mut Ui, img_rect: Rect) {
    match name.as_str() {
        "OpenLightsCore" => {
            Image::new(egui::include_image!("../assets/OpenLightsCore.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
                .paint_at(ui, img_rect);
        },
        "OpenLightsManager" => {
            Image::new(egui::include_image!("../assets/OpenLightsManager.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
                .paint_at(ui, img_rect);
        },
        "BeatMaker" => {
            Image::new(egui::include_image!("../assets/BeatMaker.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
                .paint_at(ui, img_rect);
        },
        _ => {}
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub struct Version {
    major: i8,
    minor: i8,
    patch: i32,
    release: bool,
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.minor == self.minor && self.patch == self.patch
    }
}

impl Version {
    pub fn compare(version_1: Version, version_2: Version) -> Version {
        if version_1.major > version_2.major {
            version_1
        } else if version_2.major > version_1.major {
            version_2
        } else if version_1.minor > version_2.minor {
            version_1
        } else if version_2.minor > version_1.minor {
            version_2
        } else if version_1.patch > version_2.patch {
            version_1
        } else if version_2.patch > version_1.patch {
            version_2
        } else {
            version_1 // They are actually the same version
        }
    }

    pub fn is_old(&mut self, other_ver: Version) -> bool {
        let outcome = Version::compare(self.clone(), other_ver);
        outcome == other_ver
    }

    pub fn as_string(&mut self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    pub fn from_string(ver: String, prerelease: bool) -> Version {
        let parts: Vec<&str> = ver.split('.').collect();
        let major = i8::from_str(parts[0]).unwrap();
        let minor = i8::from_str(parts[1]).unwrap();
        let patch= i32::from_str(parts[3]).unwrap();
        Version {
            major,
            minor,
            patch,
            release: !prerelease,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct GithubData {
    description: String,
    pub(crate) releases_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct ReleaseData {
    pub tag_name: String,
    pub prerelease: bool,
    pub id: i32,
    assets: Vec<AssetData>,
}

impl ReleaseData {
    pub fn clone(&mut self) -> Self {
        ReleaseData {
            tag_name: self.tag_name.clone(),
            prerelease: self.prerelease,
            id: self.id,
            assets: self.assets.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AssetData {
    size: i32,
    browser_download_url: String,
}

#[derive(Clone)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub timer: Timer,
    pub id: i32,
}

fn show_notification(ctx: &Context, notifications: &mut VecDeque<Notification>, theme: &Theme) {
    if !notifications.is_empty() {
        let screen_size = ctx.screen_rect();
        let notification_size = Vec2 { x: 300.0, y: 100.0 };
        let mut notification_pos =
            screen_size.max - egui::vec2(notification_size.x + 15.0, notification_size.y + 15.0);
        let mut notifications_clone = notifications.clone();

        for (index, notification) in notifications_clone.iter_mut().enumerate() {
            if index > 2 {
                notifications.remove(index);
                continue;
            }

            let frame = Frame {
                inner_margin: Default::default(),
                outer_margin: Default::default(),
                rounding: Rounding::from(16.),
                shadow: Default::default(),
                fill: theme.notification,
                stroke: Stroke::new(2., theme.outline),
            };

            egui::Window::new(format!("Notification{}", notification.id))
                .title_bar(false)
                .fixed_pos(notification_pos)
                .resizable(false)
                .collapsible(false)
                .movable(false)
                .frame(frame)
                .show(ctx, |ui| {
                    ui.set_min_size(notification_size);

                    ui.horizontal(|ui| {
                        ui.add_space(15.);
                        ui.add_sized(
                            Vec2 { x: 300.0, y: 20.0 },
                            egui::Label::new(
                                RichText::new(&notification.title).color(theme.text).text_style(Body).strong(),
                            ),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.add_space(30.);
                        ui.add_sized(
                            Vec2 { x: 260.0, y: 20.0 },
                            egui::Label::new(
                                RichText::new(&notification.message)
                                    .text_style(notification_font())
                                    .color(theme.text)
                                    .strong(),
                            ).wrap(),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.add_space(65.);
                        if ui
                            .add_sized(
                                Vec2 { x: 200.0, y: 10.0 },
                                egui::Button::new(
                                    RichText::new("Close")
                                        .text_style(notification_font())
                                        .color(theme.text)
                                        .strong(),
                                ).fill(theme.button),
                            )
                            .clicked()
                        {
                            notifications.remove(index);
                        }
                    });
                });

            notification_pos.y -= notification_size.y + 20.0;

            if notification.timer.update() {
                notifications.remove(index);
            }
        }
    }
}

#[derive(Clone)]
pub struct Timer {
    pub start_time: Instant,
    pub duration: Duration,
}

impl Timer {
    pub(crate) fn new(duration: Duration) -> Self {
        Self {
            start_time: Instant::now(),
            duration,
        }
    }

    fn update(&mut self) -> bool {
        let current_time = Instant::now();
        let elapsed_time = current_time.duration_since(self.start_time);
        elapsed_time >= self.duration
    }
}