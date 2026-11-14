//! Branded startup screen.
//!
//! The screen is an overlay over the already-initialized application. It never
//! blocks work on a background thread and it can be skipped with a click or
//! Space. The fade-out is part of the normal frame loop, so startup remains fast
//! even when the initial project is large.

use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Id, LayerId, Order, Pos2, Rect, Stroke, TextureHandle, Vec2};

use crate::branding;

const MIN_VISIBLE: Duration = Duration::from_millis(650);
const SKIP_AFTER: Duration = Duration::from_millis(350);
const FADE: Duration = Duration::from_millis(280);
const TOTAL: Duration = Duration::from_millis(930);

pub struct Splash {
    started: Instant,
    logo: Option<TextureHandle>,
    warning: Option<String>,
}

impl Splash {
    pub fn new(started: Instant, logo: Option<TextureHandle>, warning: Option<String>) -> Self {
        Self {
            started,
            logo,
            warning,
        }
    }

    pub fn is_active(&self) -> bool {
        self.started.elapsed() < TOTAL
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let elapsed = self.started.elapsed();
        if elapsed >= TOTAL {
            return;
        }

        let skip = elapsed >= SKIP_AFTER
            && ctx.input(|input| {
                input.pointer.any_click()
                    || input.key_pressed(egui::Key::Space)
                    || input.key_pressed(egui::Key::Escape)
            });
        if skip {
            // Skipping starts the normal fade instead of cutting to the editor.
            // This honours the 350 ms threshold while keeping the transition
            // smooth and bounded by the existing total lifetime.
            let fade_start = TOTAL - FADE;
            if elapsed < fade_start {
                self.started = Instant::now() - fade_start;
            }
            ctx.request_repaint();
            return;
        }

        let alpha = if elapsed < TOTAL - FADE {
            1.0
        } else {
            let remaining = (TOTAL - elapsed).as_secs_f32() / FADE.as_secs_f32();
            remaining.clamp(0.0, 1.0)
        };
        let time = ctx.input(|input| input.time) as f32;
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("nova-splash")));
        let center = screen.center();

        let bg = Color32::from_rgba_unmultiplied(4, 7, 15, (255.0 * alpha) as u8);
        painter.rect_filled(screen, 0.0, bg);

        // Slow cyan/violet orbs keep the background alive without competing with
        // the logo. Their motion is deliberately subtle.
        draw_glow(
            &painter,
            Pos2::new(
                screen.left() + screen.width() * (0.22 + 0.025 * time.sin()),
                screen.top() + screen.height() * 0.24,
            ),
            280.0,
            Color32::from_rgba_unmultiplied(34, 211, 238, (22.0 * alpha) as u8),
        );
        draw_glow(
            &painter,
            Pos2::new(
                screen.right() - screen.width() * (0.18 + 0.02 * (time * 0.8).cos()),
                screen.bottom() - screen.height() * 0.16,
            ),
            340.0,
            Color32::from_rgba_unmultiplied(139, 92, 246, (26.0 * alpha) as u8),
        );

        // Fine diagonal light lines; they are cheap and fade out completely.
        let line_color = Color32::from_rgba_unmultiplied(88, 231, 255, (18.0 * alpha) as u8);
        for index in 0..7 {
            let offset = ((time * 8.0) + index as f32 * 110.0) % 900.0;
            let x = screen.left() - 300.0 + offset;
            painter.line_segment(
                [
                    Pos2::new(x, screen.bottom() + 40.0),
                    Pos2::new(x + 520.0, screen.top() - 40.0),
                ],
                Stroke::new(1.0, line_color),
            );
        }

        let fade = |color: Color32| {
            Color32::from_rgba_unmultiplied(
                color.r(),
                color.g(),
                color.b(),
                (color.a() as f32 * alpha) as u8,
            )
        };

        let logo_size = (screen.height() * 0.25).clamp(132.0, 232.0);
        let logo_rect = Rect::from_center_size(
            Pos2::new(center.x, center.y - logo_size * 0.66),
            Vec2::splat(logo_size),
        );
        if let Some(logo) = &self.logo {
            painter.image(
                logo.id(),
                logo_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                fade(Color32::WHITE),
            );
        } else {
            painter.circle_filled(
                logo_rect.center(),
                logo_size * 0.42,
                fade(Color32::from_rgb(18, 28, 52)),
            );
            painter.text(
                logo_rect.center(),
                Align2::CENTER_CENTER,
                "N",
                FontId::proportional(logo_size * 0.5),
                fade(Color32::from_rgb(90, 235, 255)),
            );
        }

        let title_y = logo_rect.bottom() + 42.0;
        painter.text(
            Pos2::new(center.x, title_y),
            Align2::CENTER_CENTER,
            branding::app_name(),
            FontId::proportional(38.0),
            fade(Color32::from_rgb(238, 247, 255)),
        );
        painter.text(
            Pos2::new(center.x, title_y + 38.0),
            Align2::CENTER_CENTER,
            branding::tagline(),
            FontId::proportional(13.0),
            fade(Color32::from_rgb(111, 210, 235)),
        );
        painter.text(
            Pos2::new(center.x, title_y + 56.0),
            Align2::CENTER_CENTER,
            branding::audience(),
            FontId::proportional(11.0),
            fade(Color32::from_rgb(135, 146, 176)),
        );
        painter.text(
            Pos2::new(center.x, title_y + 80.0),
            Align2::CENTER_CENTER,
            format!("Version {}", branding::VERSION),
            FontId::proportional(13.0),
            fade(Color32::from_rgb(135, 146, 176)),
        );

        let bar =
            Rect::from_center_size(Pos2::new(center.x, title_y + 112.0), Vec2::new(328.0, 6.0));
        painter.rect_filled(bar, 3.0, fade(Color32::from_rgb(21, 30, 51)));
        let phase = if elapsed < MIN_VISIBLE {
            elapsed.as_secs_f32() / MIN_VISIBLE.as_secs_f32()
        } else {
            1.0
        };
        let sweep = (time * 0.65).rem_euclid(1.0);
        let fill_width = bar.width() * phase;
        let fill = Rect::from_min_size(bar.min, Vec2::new(fill_width, bar.height()));
        painter.rect_filled(fill, 3.0, fade(Color32::from_rgb(46, 220, 255)));
        let spark_x = bar.left() + bar.width() * sweep;
        painter.circle_filled(
            Pos2::new(spark_x, bar.center().y),
            2.2,
            fade(Color32::from_rgb(196, 160, 255)),
        );

        let status = if phase < 1.0 {
            "Initialisiere Studio…"
        } else {
            "Bereit"
        };
        painter.text(
            Pos2::new(center.x, bar.bottom() + 22.0),
            Align2::CENTER_CENTER,
            status,
            FontId::proportional(12.0),
            fade(Color32::from_rgb(138, 153, 180)),
        );

        if let Some(warning) = &self.warning {
            painter.text(
                Pos2::new(center.x, screen.bottom() - 58.0),
                Align2::CENTER_CENTER,
                warning,
                FontId::proportional(12.0),
                fade(Color32::from_rgb(245, 184, 92)),
            );
        }
        painter.text(
            Pos2::new(center.x, screen.bottom() - 28.0),
            Align2::CENTER_CENTER,
            format!("{}  ·  {}", branding::COMPANY, branding::COPYRIGHT),
            FontId::proportional(10.0),
            fade(Color32::from_rgb(69, 79, 102)),
        );

        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_is_time_bounded_and_finishes() {
        let fresh = Splash::new(Instant::now(), None, None);
        assert!(fresh.is_active());
        let finished = Splash::new(
            Instant::now() - TOTAL - Duration::from_millis(1),
            None,
            None,
        );
        assert!(!finished.is_active());
    }

    #[test]
    fn splash_duration_is_short_and_has_a_fade() {
        assert!(TOTAL <= Duration::from_millis(1_200));
        assert!(MIN_VISIBLE < TOTAL);
        assert!(FADE < TOTAL);
        assert!(SKIP_AFTER < MIN_VISIBLE);
    }
}

fn draw_glow(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    // egui has no radial gradient primitive; concentric translucent circles are
    // cheap and produce a nearly identical falloff at splash-screen scale.
    const STEPS: usize = 18;
    for step in 0..STEPS {
        let t = step as f32 / STEPS as f32;
        let r = radius * (1.0 - t * 0.72);
        let a = (color.a() as f32 * (1.0 - t).powf(1.7)) as u8;
        painter.circle_filled(
            center,
            r,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a),
        );
    }
}
