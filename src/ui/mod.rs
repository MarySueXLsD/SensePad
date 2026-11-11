//! The window: bars, pads, sidebar, controller bar and the pages shown over them.

mod browser;
mod controller;
mod help;
mod inspector;
mod pads;
mod settings;
mod topbar;
mod transport;
mod welcome;
pub mod widgets;

use crate::app::{Learn, Page, SensePadApp};
use crate::i18n::{l, lf};
use crate::kit::PAD_COUNT;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Frame, Id, Margin, Rect, Shadow, Stroke, StrokeKind};
use std::time::{Duration, Instant};

impl SensePadApp {
    /// Draw one frame of the whole interface.
    pub fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.tick(&ctx);
        let snap = self.snapshot.lock().unwrap().clone();

        egui::Panel::top("top")
            .frame(Frame::new().fill(surface()).inner_margin(Margin::symmetric(spi(22.0), spi(11.0))))
            .show(ui, |ui| self.top_bar(ui, &snap));
        egui::Panel::top("transport")
            .frame(Frame::new().fill(mix(surface(), bg(), 0.45)).inner_margin(Margin::symmetric(spi(22.0), spi(8.0))))
            .show(ui, |ui| self.transport_bar(ui));

        // The sidebar and the controller bar are resized by dragging their inner edge.
        let side = egui::Panel::right("inspector")
            .resizable(true)
            .default_size(self.settings.sidebar_width)
            .size_range(320.0..=760.0)
            .frame(Frame::new().fill(surface()).inner_margin(Margin::same(spi(18.0))))
            .show(ui, |ui| self.inspector(ui));
        let mut bottom_h = self.settings.bottom_height;
        if self.settings.show_controller {
            let bottom = egui::Panel::bottom("controller")
                .resizable(true)
                .default_size(self.settings.bottom_height)
                .size_range(200.0..=640.0)
                .frame(Frame::new().fill(surface()).inner_margin(Margin::symmetric(spi(22.0), spi(14.0))))
                .show(ui, |ui| self.controller_panel(ui, &snap));
            bottom_h = bottom.response.rect.height();
        }
        self.remember_panel_sizes(side.response.rect.width(), bottom_h);

        let pad = sp(24.0);
        egui::CentralPanel::no_frame().frame(Frame::new().inner_margin(Margin::same(spi(24.0)))).show(ui, |ui| {
            self.central_rect = ui.max_rect().expand(pad);
            let p = palette();
            gradient(ui.painter(), self.central_rect, p.bg, p.bg2);
            self.pads_view(ui);
        });

        self.overlays(&ctx);
        match self.page {
            Page::Settings => self.settings_page(&ctx),
            Page::Help => self.help_page(&ctx),
            Page::Welcome => self.welcome_page(&ctx, &snap),
            Page::None => {}
        }
        self.after_frame();

        let busy = self.learning.is_some()
            || self.page != Page::None
            || self.factory_installing
            || !self.toasts.is_empty()
            || self.loading.iter().any(|l| *l)
            || (0..PAD_COUNT).any(|i| self.since_hit(i) < 1.0)
            || self.audio.active_mask() != 0
            || self.audio.playing()
            || self.meter > 0.001;
        if snap.conn.is_some() || busy {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn remember_panel_sizes(&mut self, side_w: f32, bottom_h: f32) {
        if !(side_w.is_finite() && bottom_h.is_finite()) {
            return;
        }
        if (side_w - self.settings.sidebar_width).abs() > 0.5 || (bottom_h - self.settings.bottom_height).abs() > 0.5 {
            self.settings.sidebar_width = side_w;
            self.settings.bottom_height = bottom_h;
            self.settings_dirty = Some(Instant::now());
        }
    }

    /// The "press a button" card and the notifications, painted above everything.
    fn overlays(&mut self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("sensepad-overlay")));
        let accent = self.accent();
        let time = ctx.input(|i| i.time) as f32;

        if let Some(what) = self.learning.filter(|_| self.page == Page::None) {
            let area = self.central_rect;
            painter.rect_filled(area, CornerRadius::ZERO, alpha(bg(), 0.8));
            let card_r = Rect::from_center_size(area.center(), vec2(500.0_f32.min(area.width() - 24.0), 170.0));
            painter.add(Shadow { offset: [0, 12], blur: 40, spread: 0, color: Color32::from_black_alpha(160) }.as_shape(card_r, cr(18.0)));
            painter.rect(card_r, cr(18.0), surface(), Stroke::new(1.5, accent), StrokeKind::Inside);
            let pulse = 0.5 + 0.5 * (time * 4.0).sin();
            let c = pos2(card_r.left() + 58.0, card_r.center().y);
            painter.circle_filled(c, 20.0 + pulse * 8.0, alpha(accent, 0.12));
            painter.circle_filled(c, 18.0, alpha(accent, 0.25));
            painter.circle_filled(c, 7.0, accent);
            let x = card_r.left() + 104.0;
            painter.text(pos2(x, card_r.center().y - 30.0), Align2::LEFT_CENTER, l("Нажмите кнопку на геймпаде", "Press a button on the controller"), display(20.0), text());
            let line = match what {
                Learn::Pad(pad) => lf("Она будет играть «{}»", "It will play “{}”", &[&self.pad_name(pad)]),
                Learn::Func(f) => lf("Она будет делать: {}", "It will do: {}", &[&crate::app::func_name(f)]),
            };
            painter.text(pos2(x, card_r.center().y + 1.0), Align2::LEFT_CENTER, line, FontId::proportional(14.0), accent);
            painter.text(pos2(x, card_r.center().y + 28.0), Align2::LEFT_CENTER, l("Esc — отмена", "Esc cancels"), FontId::proportional(12.5), dim());
        }

        // Notifications: bottom right of the pads, or bottom centre while a page is open.
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::unique("sensepad-toasts")));
        let page_open = self.page != Page::None;
        let home = if page_open { ctx.content_rect() } else { self.central_rect };
        self.toasts.retain(|t| t.born.elapsed() < Duration::from_millis(3400));
        let mut y = home.bottom() - 18.0;
        for toast in self.toasts.iter().rev() {
            let age = toast.born.elapsed().as_secs_f32();
            let a = (age / 0.15).min(1.0) * ((3.4 - age) / 0.4).clamp(0.0, 1.0);
            let galley = painter.layout_no_wrap(toast.text.clone(), FontId::proportional(13.5), alpha(text(), a));
            let w = galley.size().x + 44.0;
            let x = if page_open { home.center().x - w * 0.5 } else { home.right() - 22.0 - w };
            let r = Rect::from_min_size(pos2(x, y - 42.0), vec2(w, 42.0));
            painter.add(Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha((110.0 * a) as u8) }.as_shape(r, cr(12.0)));
            painter.rect(r, cr(12.0), alpha(card_hi(), a), Stroke::new(1.0, alpha(border(), a)), StrokeKind::Inside);
            painter.circle_filled(pos2(r.left() + 18.0, r.center().y), 4.0, alpha(toast.color, a));
            painter.galley(pos2(r.left() + 32.0, r.center().y - galley.size().y / 2.0), galley, text());
            y -= 50.0;
        }
    }
}
