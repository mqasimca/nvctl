//! VRAM usage bar widget
//!
//! Horizontal progress bar showing VRAM usage with glossy effects.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Point, Rectangle, Renderer, Theme};
use nvctl::domain::MemoryInfo;

/// VRAM usage bar widget
pub struct VramBar {
    memory_info: MemoryInfo,
}

impl VramBar {
    /// Create a new VRAM bar
    pub fn new(memory_info: MemoryInfo) -> Self {
        Self { memory_info }
    }

    /// Get the usage ratio (0.0 - 1.0)
    fn ratio(&self) -> f32 {
        self.memory_info.usage_ratio()
    }
}

impl canvas::Program<Message> for VramBar {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let padding = 12.0;
        let bar_height = 20.0;
        let bar_width = bounds.width - padding * 2.0;
        let bar_y = bounds.height / 2.0 + 4.0;
        let bar_radius = bar_height / 2.0;

        let ratio = self.ratio();

        // Get color based on usage (cyan at low, orange/red at high)
        let main_color = vram_color(ratio);
        let bright_color = colors::glossy_highlight(main_color);

        // ═══════════════════════════════════════════════════════════════════════
        // BACKGROUND BAR - Glossy glass track
        // ═══════════════════════════════════════════════════════════════════════
        let bg_rect = rounded_rect(padding, bar_y, bar_width, bar_height, bar_radius);
        frame.fill(&bg_rect, colors::BG_ELEVATED);

        // Glass highlight on track (top edge)
        let highlight_line = Path::line(
            Point::new(padding + bar_radius, bar_y + 2.0),
            Point::new(padding + bar_width - bar_radius, bar_y + 2.0),
        );
        frame.stroke(
            &highlight_line,
            Stroke::default()
                .with_width(2.0)
                .with_color(colors::GLASS_HIGHLIGHT)
                .with_line_cap(canvas::LineCap::Round),
        );

        // ═══════════════════════════════════════════════════════════════════════
        // FILLED PORTION - Glossy colorful fill
        // ═══════════════════════════════════════════════════════════════════════
        if ratio > 0.0 {
            let fill_width = (bar_width * ratio).max(bar_height);

            // Outer soft glow
            let glow_rect = rounded_rect(
                padding - 2.0,
                bar_y - 2.0,
                fill_width + 4.0,
                bar_height + 4.0,
                bar_radius + 2.0,
            );
            frame.fill(&glow_rect, colors::with_alpha(main_color, 0.1));

            // Main fill
            let fill_rect = rounded_rect(padding, bar_y, fill_width, bar_height, bar_radius);
            frame.fill(&fill_rect, main_color);

            // Glossy highlight on top
            if fill_width > bar_height {
                let hl_line = Path::line(
                    Point::new(padding + bar_radius, bar_y + 3.0),
                    Point::new(padding + fill_width - bar_radius, bar_y + 3.0),
                );
                frame.stroke(
                    &hl_line,
                    Stroke::default()
                        .with_width(2.5)
                        .with_color(colors::with_alpha(bright_color, 0.4))
                        .with_line_cap(canvas::LineCap::Round),
                );
            }
        }

        // ═══════════════════════════════════════════════════════════════════════
        // BORDER - Clean subtle outline
        // ═══════════════════════════════════════════════════════════════════════
        let border = rounded_rect(padding, bar_y, bar_width, bar_height, bar_radius);
        frame.stroke(
            &border,
            Stroke::default()
                .with_width(1.0)
                .with_color(colors::GLASS_BORDER),
        );

        // ═══════════════════════════════════════════════════════════════════════
        // VRAM TEXT - Usage info
        // ═══════════════════════════════════════════════════════════════════════

        // "VRAM" label
        frame.fill_text(Text {
            content: "VRAM".to_string(),
            position: Point::new(padding, bar_y - 14.0),
            color: colors::TEXT_SECONDARY,
            size: 11.0.into(),
            horizontal_alignment: Horizontal::Left,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // Usage text (e.g., "2.1 / 16.0 GB")
        let usage_text = format!(
            "{:.1} / {:.1} GB",
            self.memory_info.used_gb(),
            self.memory_info.total_gb()
        );
        frame.fill_text(Text {
            content: usage_text,
            position: Point::new(bounds.width - padding, bar_y - 14.0),
            color: main_color,
            size: 12.0.into(),
            horizontal_alignment: Horizontal::Right,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // Percentage in center of bar
        let percent_text = format!("{}%", self.memory_info.usage_percent());
        frame.fill_text(Text {
            content: percent_text,
            position: Point::new(bounds.width / 2.0, bar_y + bar_height / 2.0),
            color: if ratio > 0.3 {
                colors::TEXT_PRIMARY
            } else {
                colors::TEXT_SECONDARY
            },
            size: 11.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        vec![frame.into_geometry()]
    }
}

/// Get color based on VRAM usage
fn vram_color(ratio: f32) -> iced::Color {
    if ratio < 0.5 {
        // Cyan at low usage
        colors::ACCENT_CYAN
    } else if ratio < 0.75 {
        // Transition to orange
        let t = (ratio - 0.5) * 4.0;
        colors::lerp(colors::ACCENT_CYAN, colors::ACCENT_ORANGE, t)
    } else {
        // Transition to red at high usage
        let t = (ratio - 0.75) * 4.0;
        colors::lerp(colors::ACCENT_ORANGE, colors::ACCENT_RED, t)
    }
}

/// Create a rounded rectangle path
fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Path {
    Path::new(|builder| {
        let r = radius.min(width / 2.0).min(height / 2.0);

        builder.move_to(Point::new(x + r, y));
        builder.line_to(Point::new(x + width - r, y));
        builder.arc_to(Point::new(x + width, y), Point::new(x + width, y + r), r);
        builder.line_to(Point::new(x + width, y + height - r));
        builder.arc_to(
            Point::new(x + width, y + height),
            Point::new(x + width - r, y + height),
            r,
        );
        builder.line_to(Point::new(x + r, y + height));
        builder.arc_to(Point::new(x, y + height), Point::new(x, y + height - r), r);
        builder.line_to(Point::new(x, y + r));
        builder.arc_to(Point::new(x, y), Point::new(x + r, y), r);
        builder.close();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vram_bar_ratio() {
        // 4 GB used of 16 GB = 25%
        let bar = VramBar::new(MemoryInfo::new(
            16 * 1024 * 1024 * 1024,
            4 * 1024 * 1024 * 1024,
            12 * 1024 * 1024 * 1024,
        ));
        assert!((bar.ratio() - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_vram_bar_empty() {
        let bar = VramBar::new(MemoryInfo::default());
        assert_eq!(bar.ratio(), 0.0);
    }
}
