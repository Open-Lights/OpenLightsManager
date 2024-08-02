#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use egui::Vec2;
use OpenLightsManager::gui;

fn main() -> eframe::Result<()> {

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_close_button(true)
            .with_maximize_button(false)
            .with_minimize_button(false)
            .with_resizable(false)
            .with_title_shown(true)
            .with_visible(true)
            .with_inner_size(Vec2 {x: 600., y: 600.})
            .with_icon(
                eframe::icon_data::from_png_bytes(&include_bytes!("../assets/icon.ico")[..])
                    .expect("Failed to load icon"),
            ),
        ..Default::default()
    };

    eframe::run_native(
        "Open Lights Manager",
        native_options,
        Box::new(move |cc| {
            cc.egui_ctx
                .send_viewport_cmd(egui::viewport::ViewportCommand::Visible(true));
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(gui::OpenLightsManager::new(&cc.egui_ctx)))
        }),
    )
}