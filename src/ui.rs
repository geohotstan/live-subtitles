use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use crate::app::{SharedCaption, SharedOutputLanguage};
use crate::config::OutputLanguage;
use eframe::egui;

pub fn run_overlay(
    captions: SharedCaption,
    output_language: SharedOutputLanguage,
    stop: Arc<AtomicBool>,
    font_size: f32,
    overlay_width_frac: f32,
) -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Subtitles")
            .with_always_on_top()
            .with_transparent(true)
            .with_decorations(false)
            .with_resizable(true)
            .with_inner_size([900.0, 160.0]),
        ..Default::default()
    };

    let app = OverlayApp {
        captions,
        output_language,
        stop,
        font_size,
        overlay_width_frac: overlay_width_frac.clamp(0.1, 1.0),
        last_text: String::new(),
        show_controls: true,
        layout_cache: None,
    };

    eframe::run_native(
        "Subtitles",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

struct OverlayApp {
    captions: SharedCaption,
    output_language: SharedOutputLanguage,
    stop: Arc<AtomicBool>,
    font_size: f32,
    overlay_width_frac: f32,
    last_text: String,
    show_controls: bool,
    layout_cache: Option<LayoutCache>,
}

struct LayoutCache {
    text: String,
    wrap_width_px: u32,
    max_height_px: u32,
    max_font_size_px: u32,
    galley: std::sync::Arc<egui::Galley>,
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.stop.store(true, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::S)) {
            self.show_controls = !self.show_controls;
        }

        // Allow dragging the window by clicking anywhere.
        // When the control bar is visible, require `Alt` to avoid fighting with UI widgets.
        if ctx.input(|i| {
            i.pointer.primary_pressed() && (!self.show_controls || i.modifiers.alt)
        }) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let (text, updated_at) = self.captions.snapshot();
        let age = updated_at.elapsed();
        let visible = age <= Duration::from_secs(6);

        if visible {
            self.last_text = text;
        }

        ctx.request_repaint_after(Duration::from_millis(16));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                if self.show_controls {
                    egui::TopBottomPanel::top("controls")
                        .frame(
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 120))
                                .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
                        )
                        .show_inside(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Output:");

                                let mut selected = self.output_language.get();
                                egui::ComboBox::from_id_salt("output_language")
                                    .selected_text(match selected {
                                        OutputLanguage::Original => "Original",
                                        OutputLanguage::English => "English",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut selected,
                                            OutputLanguage::English,
                                            "English",
                                        );
                                        ui.selectable_value(
                                            &mut selected,
                                            OutputLanguage::Original,
                                            "Original",
                                        );
                                    });

                                if selected != self.output_language.get() {
                                    self.output_language.set(selected);
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label("Alt+drag: move • S: controls • Esc: quit");
                                    },
                                );
                            });
                        });
                }

                let rect = ui.available_rect_before_wrap();
                let available_width = rect.width() * self.overlay_width_frac;
                let subtitle_rect = rect.shrink2(egui::vec2(
                    (rect.width() - available_width) * 0.5,
                    16.0,
                ));

                let text = if visible {
                    self.last_text.clone()
                } else {
                    String::new()
                };
                let painter = ui.painter_at(rect);

                // Layout + scale text so it fits in the subtitle box.
                let wrap_width = subtitle_rect.width().max(1.0);
                let max_height = subtitle_rect.height().max(1.0);
                let galley =
                    self.layout_subtitle_galley(&painter, text.as_str(), wrap_width, max_height);

                let align = egui::Align2::CENTER_CENTER;
                let galley_rect = align.anchor_size(subtitle_rect.center(), galley.size());

                // Simple shadow for readability.
                let shadow_color = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 170);
                let fg = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 240);

                for (dx, dy) in [(-2.0, 0.0), (2.0, 0.0), (0.0, -2.0), (0.0, 2.0)] {
                    painter.galley_with_override_text_color(
                        galley_rect.min + egui::vec2(dx, dy),
                        galley.clone(),
                        shadow_color,
                    );
                }
                painter.galley_with_override_text_color(galley_rect.min, galley, fg);
            });
    }
}

impl OverlayApp {
    fn layout_subtitle_galley(
        &mut self,
        painter: &egui::Painter,
        text: &str,
        wrap_width: f32,
        max_height: f32,
    ) -> std::sync::Arc<egui::Galley> {
        let wrap_width_px = wrap_width.round().max(1.0) as u32;
        let max_height_px = max_height.round().max(1.0) as u32;
        let max_font_size_px = self.font_size.round().max(1.0) as u32;

        if let Some(cache) = &self.layout_cache {
            if cache.text == text
                && cache.wrap_width_px == wrap_width_px
                && cache.max_height_px == max_height_px
                && cache.max_font_size_px == max_font_size_px
            {
                return cache.galley.clone();
            }
        }

        let galley = layout_text_fit(painter, text, wrap_width, max_height, self.font_size);
        self.layout_cache = Some(LayoutCache {
            text: text.to_string(),
            wrap_width_px,
            max_height_px,
            max_font_size_px,
            galley: galley.clone(),
        });
        galley
    }
}

fn layout_text_fit(
    painter: &egui::Painter,
    text: &str,
    wrap_width: f32,
    max_height: f32,
    max_font_size: f32,
) -> std::sync::Arc<egui::Galley> {
    if text.trim().is_empty() {
        return painter.layout(
            String::new(),
            egui::FontId::proportional(max_font_size),
            egui::Color32::WHITE,
            wrap_width,
        );
    }

    let min_font: f32 = 12.0;
    let mut lo = min_font.min(max_font_size);
    let mut hi = max_font_size.max(min_font);
    let mut best = lo;

    // Try a small binary search for the largest font that still fits vertically.
    for _ in 0..10 {
        let mid = (lo + hi) * 0.5;
        let galley = painter.layout(
            text.to_string(),
            egui::FontId::proportional(mid),
            egui::Color32::WHITE,
            wrap_width,
        );

        if galley.size().y <= max_height {
            best = mid;
            lo = mid;
        } else {
            hi = mid;
        }
    }

    painter.layout(
        text.to_string(),
        egui::FontId::proportional(best),
        egui::Color32::WHITE,
        wrap_width,
    )
}
