use std::cmp::PartialEq;
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::{fs, i32};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, mpsc};
use std::sync::atomic::{AtomicI8, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{CentralPanel, Color32, Context, FontFamily, FontId, Frame, Image, Pos2, pos2, ProgressBar, Rect, RichText, Rounding, Stroke, TextStyle, Ui, Vec2};
use egui::TextStyle::Body;
use egui_file::FileDialog;
use serde::{Deserialize, Serialize};

use crate::io::{AppEvents, check_for_all_updates, download_application, gather_app_data, get_installation_data, launch_application, save_app_data_offline, should_check_github, update};
use crate::notifications::{app_installation_failure, app_installation_success, java_failure_corrupted, java_failure_invalid, java_failure_issue, java_success, manager_installation_success, rate_limit_notification};
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
                #[allow(deprecated)]
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
    ui.style_mut().visuals.extreme_bg_color = open_lights_manager.theme.button;
}

impl OpenLightsManager {
    pub fn new(ctx: &Context) -> Self {
        configure_text_styles(ctx);

        let mut settings = load_settings();
        let mut notifications = VecDeque::new();
        let apps_pre = gather_app_data(settings.unstable_releases, &mut settings);
        if let Some(notification) = apps_pre.1 {
            notify(ctx, notification, &mut notifications);
        };
        let apps = apps_pre.0;
        let theme = Theme::get_theme(&settings);
        let file_explorer = FileExplorer {
            opened_file: None,
            open_file_dialog: None,
        };

        OpenLightsManager {
            current_screen: Screen::default(),
            notifications,
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
        self.render_app_panel(ui, true);
    }

    pub fn render_browse(&mut self, ui: &mut Ui) {
        let rect = Self::tab_area();
        ui.painter().rect(rect, Rounding::same(16.), self.theme.panel, Stroke::NONE);
        self.render_app_panel(ui, false);
    }

    pub fn render_settings(&mut self, ui: &mut Ui) {
        let rect = Self::tab_area();
        ui.painter().rect(rect, Rounding::same(16.), self.theme.panel, Stroke::NONE);
        self.render_settings_panel(ui);
    }

    fn render_app_panel(&mut self, ui: &mut Ui, install_only: bool) {
        let rect = Self::scroll_area();

        ui.allocate_ui_at_rect(rect, |ui| {
            egui::ScrollArea::vertical()
                .max_height(420.)
                .max_width(550.)
                .show(ui, |ui| {
                    for app in self.apps.iter_mut(){
                        if (install_only && app.installed) || (!install_only && !app.installed) {
                            app.render(ui, &self.theme, &mut self.notifications, &mut self.settings);
                            ui.add_space(10.);
                        }
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
                        egui::ScrollArea::horizontal()
                            .max_width(200.)
                            .show(ui, |ui| {
                                ui.add_sized([200., 50.], egui::Label::new(RichText::new(&self.settings.jvm_path).color(self.theme.text).text_style(notification_font())));
                            });
                        if ui.add_sized([50., 30.], egui::Button::new(RichText::new("Locate").color(self.theme.text))).clicked() {
                            self.file_explorer.open();
                        }
                        if ui.add_sized([50., 30.], egui::Button::new(RichText::new("Check").color(self.theme.text))).clicked() {
                            let path = Path::new(&self.settings.jvm_path);
                            let filename = path.file_stem().unwrap().to_string_lossy().to_string();

                            if filename != "java" && filename != "javaw" {
                                let notification = java_failure_invalid();
                                notify(ui.ctx(), notification, &mut self.notifications);
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
                                            let notification = java_success(stdout_as_string.to_string());
                                            notify(ui.ctx(), notification, &mut self.notifications);
                                        } else {
                                            let notification = java_failure_corrupted();
                                            notify(ui.ctx(), notification, &mut self.notifications);
                                        }
                                    }
                                    Err(_e) => {
                                        let notification = java_failure_issue();
                                        notify(ui.ctx(), notification, &mut self.notifications);
                                    }
                                }
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.add_sized([100., 50.], egui::Label::new(RichText::new("Github Token: ").color(self.theme.text)));
                        if ui.add_sized([250., 30.], egui::TextEdit::singleline(&mut self.settings.github_token).hint_text("Ex: <insert_example>").text_color(self.theme.text)).lost_focus() {
                            self.settings.save_settings();
                        };
                        ui.add_sized([50., 30.], egui::Hyperlink::from_label_and_url(RichText::new("Help").color(Color32::BLUE).underline(), "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens#creating-a-fine-grained-personal-access-token"));
                    });

                    ui.horizontal(|ui| {
                        ui.add_sized([100., 50.], egui::Label::new(RichText::new("Override Rate Limiter").color(self.theme.text)));
                        if ui.add_sized([50., 50.], egui::Checkbox::without_text(&mut self.settings.override_rate_limit)).clicked() {
                            self.settings.save_settings();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.add_sized([100., 50.], egui::Label::new(RichText::new("Last Update Check: ").color(self.theme.text)));
                        ui.add_sized([100., 50.], egui::Label::new(RichText::new(&self.settings.last_github_check_formatted).color(self.theme.text)));
                        if ui.add_sized([25., 25.], egui::Button::new(RichText::new("↻").color(self.theme.text))).clicked() {
                            if should_check_github(&self.settings) {
                                check_for_all_updates(&mut self.apps, self.settings.unstable_releases, &mut self.settings)
                            } else {
                                let notification = rate_limit_notification();
                                notify(ui.ctx(), notification, &mut self.notifications);
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
        CentralPanel::default().show(ctx, |_ui| {
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

#[derive(Debug)]
pub struct ThreadCommunication {
    pub event_receiver: Receiver<(AppEvents, Option<String>)>,
    pub event_sender: Sender<(AppEvents, Option<String>)>,
}

impl Default for ThreadCommunication {
    fn default() -> Self {
        let (event_sender, event_receiver) = mpsc::channel();
        ThreadCommunication {
            event_sender,
            event_receiver,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct App {
    pub installed: bool,
    #[serde(skip)]
    pub event: AppEvents,
    pub(crate) name: String,
    pub path: String, // This is the app_data path
    pub app_path: String, // This is the executable path
    pub version: String,
    pub(crate) image_url: String,
    pub github_repo: String,
    pub(crate) github_data: GithubData,
    pub(crate) release_data: ReleaseData,
    pub has_update: bool,
    pub update_download_url: Option<String>,
    pub(crate) launchable: bool,
    #[serde(skip)]
    pub progress: Arc<AtomicI8>,
    #[serde(skip)]
    pub thread_communication: ThreadCommunication,
    #[serde(skip)]
    pub process: Arc<AtomicU32>,
}

impl App {
    pub fn default(
        name: String,
        path: String,
        version: String,
        image_url: String,
        github_repo: String,
        github_data: GithubData,
        release_data: ReleaseData,
        has_update: bool,
        launchable: bool,
    ) -> Self {
        App {
            installed: false,
            event: AppEvents::default(),
            name,
            path,
            app_path: String::new(),
            version,
            image_url,
            github_repo,
            github_data,
            release_data,
            has_update,
            update_download_url: None,
            launchable,
            progress: Arc::new(AtomicI8::new(0)),
            thread_communication: ThreadCommunication::default(),
            process: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, ui: &mut Ui, theme: &Theme, notifications: &mut VecDeque<Notification>, settings: &mut Settings) {

        if let Ok(event) = self.thread_communication.event_receiver.try_recv() {
            match event.0 {
                AppEvents::Failed => {
                    self.event = AppEvents::None;
                    let notification = app_installation_failure(&self.name);
                    notify(ui.ctx(), notification, notifications);
                }
                AppEvents::JavaInstalled => {
                    println!("Java Installed!");
                    let path_str = format!("openlightsmanager/apps/{}/{}", self.name, event.1.unwrap());
                    let path = Path::new(&path_str);
                    let full_path = fs::canonicalize(path).unwrap();
                    let full_path_str = full_path.to_string_lossy().to_string();
                    settings.jvm_path = full_path_str;
                    settings.save_settings();
                    self.installed = true;
                    self.event = AppEvents::None;
                    save_app_data_offline(self);
                    let notification = app_installation_success(&self.name);
                    notify(ui.ctx(), notification, notifications);
                }
                AppEvents::ManagerInstalled => {
                    println!("Manager Installed!");
                    self.installed = true;
                    self.event = AppEvents::None;
                    save_app_data_offline(self);
                    let notification = manager_installation_success();
                    notify(ui.ctx(), notification, notifications);
                }
                AppEvents::AppInstalled => {
                    println!("App Installed!");
                    self.installed = true;
                    self.has_update = false;
                    self.event = AppEvents::None;
                    save_app_data_offline(self);
                    let notification = app_installation_success(&self.name);
                    notify(ui.ctx(), notification, notifications);
                }
                _ => {
                    self.event = event.0;
                }
            }
        }

        let installation_data = get_installation_data(&self);
        ui.allocate_ui(Vec2::from([550., 140.]), |ui| {
            ui.horizontal(|ui| {
                // Image
                let image = app_image(&self.name);
                ui.add_sized([100., 100.], image);

                let installing = self.event == AppEvents::Downloading || self.event == AppEvents::Extracting;
                ui.vertical(|ui| {
                   ui.horizontal(|ui| {
                       let name = if self.installed {
                           format!("{} {}", &self.name, &self.version)
                       } else {
                           self.name.clone()
                       };
                       // Name
                       ui.add_sized([320., 40.], egui::Label::new(RichText::new(name).color(theme.text).strong()));

                       // Action Button
                       let action_button_text = if self.installed {
                           match self.event {
                               AppEvents::Running => "Kill".to_string(),
                               _ => "Launch".to_string(),
                           }
                       } else {
                           match self.event {
                               AppEvents::Downloading => "Downloading".to_string(),
                               AppEvents::Extracting => "Extracting".to_string(),
                               _ => "Install".to_string(),
                           }
                       };

                       if self.installed && self.has_update {
                           ui.add_enabled_ui(!installing, |ui| {
                               if installation_data.launchable && ui.add_sized([50., 40.], egui::Button::new(RichText::new(action_button_text).color(theme.text)).fill(theme.button)).clicked() {
                                   let notification = launch_application(self, &settings.jvm_path);
                                   notify(ui.ctx(), notification, notifications);
                               }

                               if ui.add_sized([50., 40.], egui::Button::new(RichText::new("Update").color(theme.text)).fill(theme.button)).clicked() {
                                   // TODO Update
                                   update(&self, &self.progress, self.thread_communication.event_sender.clone());
                               }
                           });
                       } else {
                           ui.add_enabled_ui(!installing, |ui| {
                               if (installation_data.launchable && self.installed) && ui.add_sized([100., 40.], egui::Button::new(RichText::new(action_button_text).color(theme.text)).fill(theme.button)).clicked() {
                                   if self.installed {
                                       let notification = launch_application(self, &settings.jvm_path);
                                       notify(ui.ctx(), notification, notifications);
                                   } else {
                                       self.event = AppEvents::Downloading;
                                       download_application(self, &self.progress, self.thread_communication.event_sender.clone());
                                   }
                               }
                           });
                       }
                   });
                   ui.horizontal(|ui| {
                       // Description
                       ui.add_sized([320., 40.], egui::Label::new(RichText::new(&self.github_data.description).color(theme.text).text_style(notification_font())));

                       // Action Button / Progress Bar
                       if !installing {
                           if self.installed {
                               if ui.add_sized([100., 40.], egui::Button::new(RichText::new("Uninstall").color(theme.text)).fill(theme.button)).clicked() {
                                   let path_str = format!("openlightsmanager/apps/{}/", self.name);
                                   let path = Path::new(&path_str);
                                   let executable_path_str = get_full_path_str(&self.name, &installation_data.app_path);
                                   if path.exists() {
                                       fs::remove_dir_all(path).unwrap();
                                       if executable_path_str == settings.jvm_path {
                                           settings.jvm_path.clear();
                                           settings.save_settings();
                                       }
                                   }
                                   self.installed = false;
                                   save_app_data_offline(&self);
                               }
                           }
                       } else {
                           let prgs = self.progress.load(Ordering::Relaxed);
                           ui.ctx().request_repaint_after(Duration::from_millis(10));
                           ui.add_sized([100., 30.], ProgressBar::new(prgs as f32 / 100.));
                       }
                   });
                });
            });
        });
    }
}

fn get_full_path_str(name: &String, executable: &String) -> String {
    let path_str = format!("openlightsmanager/apps/{}{}", name, executable);
    let path = Path::new(&path_str);
    let full_path = fs::canonicalize(path).unwrap();
    full_path.to_string_lossy().to_string()
}

fn app_image(name: &String) -> Image<'_> {
    match name.as_str() {
        "OpenLightsCore" => {
            Image::new(egui::include_image!("../assets/OpenLightsCore.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
        },
        "OpenLightsManager" => {
            Image::new(egui::include_image!("../assets/OpenLightsManager.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
        },
        "BeatMaker" => {
            Image::new(egui::include_image!("../assets/BeatMaker.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})

        },
        "graalvm-ce-builds" => {
            Image::new(egui::include_image!("../assets/graalvm-ce-builds.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
        },
        "BeatFileEditor" => {
            Image::new(egui::include_image!("../assets/BeatFileEditor.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
        },
        "Christmas-Jukebox" => {
            Image::new(egui::include_image!("../assets/Christmas-Jukebox.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
        },
        _ => {
            Image::new(egui::include_image!("../assets/Unknown.png"))
                .fit_to_exact_size(Vec2 {x: 100., y: 100.})
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GithubData {
    pub(crate) description: String,
    pub archived: bool,
    pub(crate) releases_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReleaseData {
    pub tag_name: String,
    pub prerelease: bool,
    pub id: i32,
    pub assets: Vec<AssetData>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetData {
    pub size: i32,
    pub browser_download_url: String,
}

#[derive(Clone)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub timer: Timer,
    pub id: i32,
}

fn notify(ctx: &Context, notification: Notification, notifications: &mut VecDeque<Notification>) {
    notifications.push_front(notification);
    ctx.request_repaint_after(Duration::from_millis(10));
}

fn show_notification(ctx: &Context, notifications: &mut VecDeque<Notification>, theme: &Theme) {
    if !notifications.is_empty() {
        ctx.request_repaint_after(Duration::from_secs(1));
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