//! SensePad — a DualSense groovebox.
//! Author: MarySue.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod controller_art;
mod controller_skin;
mod controller_view;
mod dualsense;
mod factory;
mod i18n;
mod icons;
mod input;
mod kit;
mod library;
mod naming;
#[cfg(test)]
mod packtool;
#[cfg(test)]
mod preview;
mod seq;
mod theme;
mod ui;

use eframe::egui;
use std::sync::Arc;

struct Shell(app::SensePadApp);

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.frame(ui);
    }
}

fn main() -> eframe::Result {
    let settings = kit::load_settings();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("SensePad — {}", settings.profile))
            .with_app_id("sensepad")
            .with_inner_size([1500.0, 980.0])
            .with_min_inner_size([1180.0, 760.0])
            .with_icon(Arc::new(theme::app_icon())),
        ..Default::default()
    };
    eframe::run_native("SensePad", options, Box::new(|cc| Ok(Box::new(Shell(app::SensePadApp::new(cc))))))
}
