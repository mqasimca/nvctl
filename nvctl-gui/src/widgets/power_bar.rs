//! Power bar widget
//!
//! Glossy horizontal bar with vibrant colors showing power usage vs limit.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Point, Rectangle, Renderer, Theme};
use nvctl::domain::{PowerConstraints, PowerLimit};

/// Power bar widget with neon glow effect
pub struct PowerBar {
    usage: PowerLimit,
    limit: PowerLimit,
    constraints: Option<PowerConstraints>,
}

impl PowerBar {
    /// Create a new power bar
    pub fn new(
        usage: PowerLimit,
        limit: PowerLimit,
        constraints: Option<PowerConstraints>,
    ) -> Self {
        Self {
            usage,
            limit,
            constraints,
        }
    }

    /// Get the power usage ratio
    fn ratio(&self) -> f32 {
        if self.limit.as_watts() == 0 {
            return 0.0;
        }
        (self.usage.as_watts() as f32 / self.limit.as_watts() as f32).clamp(0.0, 1.0)
    }
}

impl canvas::Program<Message> for PowerBar {
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

        let padding = 15.0;
        let bar_height = 24.0;
        let bar_width = bounds.width - padding * 2.0;
        let bar_y = bounds.height / 2.0 + 8.0;
        let bar_radius = bar_height / 2.0;

        let ratio = self.ratio();
        // Use smooth gradient based on power ratio
        let main_color = colors::lerp(colors::ACCENT_GREEN, colors::ACCENT_RED, ratio);
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
                padding - 3.0,
                bar_y - 3.0,
                fill_width + 6.0,
                bar_height + 6.0,
                bar_radius + 3.0,
            );
            frame.fill(&glow_rect, colors::with_alpha(main_color, 0.12));

            // Main fill
            let fill_rect = rounded_rect(padding, bar_y, fill_width, bar_height, bar_radius);
            frame.fill(&fill_rect, main_color);

            // Glossy highlight on top
            if fill_width > bar_height {
                let hl_line = Path::line(
                    Point::new(padding + bar_radius, bar_y + 4.0),
                    Point::new(padding + fill_width - bar_radius, bar_y + 4.0),
                );
                frame.stroke(
                    &hl_line,
                    Stroke::default()
                        .with_width(3.0)
                        .with_color(colors::with_alpha(bright_color, 0.5))
                        .with_line_cap(canvas::LineCap::Round),
                );
            }

            // End cap orb
            let end_x = padding + fill_width - bar_radius;
            let end_y = bar_y + bar_height / 2.0;

            // Outer glow
            let end_glow_outer = Path::circle(Point::new(end_x, end_y), 16.0);
            frame.fill(&end_glow_outer, colors::with_alpha(main_color, 0.15));

            // Core orb
            let end_core = Path::circle(Point::new(end_x, end_y), 8.0);
            frame.fill(&end_core, main_color);

            // Glossy highlight spot
            let end_highlight = Path::circle(Point::new(end_x - 2.0, end_y - 2.0), 3.5);
            frame.fill(&end_highlight, colors::with_alpha(bright_color, 0.6));
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
        // POWER TEXT - Clean display above bar
        // ═══════════════════════════════════════════════════════════════════════
        let power_text = format!("{}W", self.usage.as_watts());
        frame.fill_text(Text {
            content: power_text,
            position: Point::new(padding, bar_y - 26.0),
            color: colors::TEXT_PRIMARY,
            size: 30.0.into(),
            horizontal_alignment: Horizontal::Left,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // Limit text in accent color
        let limit_text = format!("/ {}W", self.limit.as_watts());
        frame.fill_text(Text {
            content: limit_text,
            position: Point::new(padding + 68.0, bar_y - 26.0),
            color: colors::with_alpha(main_color, 0.7),
            size: 14.0.into(),
            horizontal_alignment: Horizontal::Left,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // Label below bar in accent color
        frame.fill_text(Text {
            content: "POWER".to_string(),
            position: Point::new(padding, bar_y + bar_height + 10.0),
            color: colors::with_alpha(main_color, 0.8),
            size: 11.0.into(),
            horizontal_alignment: Horizontal::Left,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // ═══════════════════════════════════════════════════════════════════════
        // DEFAULT MARKER - Clean indicator
        // ═══════════════════════════════════════════════════════════════════════
        if let Some(ref constraints) = self.constraints {
            if constraints.max.as_watts() > 0 {
                let default_ratio =
                    constraints.default.as_watts() as f32 / constraints.max.as_watts() as f32;
                let marker_x = padding + bar_width * default_ratio;

                // Marker line with glow
                let marker_glow = Path::line(
                    Point::new(marker_x, bar_y - 4.0),
                    Point::new(marker_x, bar_y + bar_height + 4.0),
                );
                frame.stroke(
                    &marker_glow,
                    Stroke::default()
                        .with_width(4.0)
                        .with_color(colors::with_alpha(colors::ACCENT_GOLD, 0.2)),
                );

                let marker = Path::line(
                    Point::new(marker_x, bar_y - 2.0),
                    Point::new(marker_x, bar_y + bar_height + 2.0),
                );
                frame.stroke(
                    &marker,
                    Stroke::default()
                        .with_width(2.0)
                        .with_color(colors::ACCENT_GOLD)
                        .with_line_cap(canvas::LineCap::Round),
                );

                // Diamond marker at top
                let diamond = Path::new(|builder| {
                    builder.move_to(Point::new(marker_x, bar_y - 8.0));
                    builder.line_to(Point::new(marker_x + 4.0, bar_y - 4.0));
                    builder.line_to(Point::new(marker_x, bar_y));
                    builder.line_to(Point::new(marker_x - 4.0, bar_y - 4.0));
                    builder.close();
                });
                frame.fill(&diamond, colors::ACCENT_GOLD);
            }
        }

        vec![frame.into_geometry()]
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
    fn test_power_bar_ratio() {
        let bar = PowerBar::new(
            PowerLimit::from_watts(150),
            PowerLimit::from_watts(300),
            None,
        );
        assert!((bar.ratio() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_power_bar_zero_limit() {
        let bar = PowerBar::new(PowerLimit::from_watts(100), PowerLimit::from_watts(0), None);
        assert_eq!(bar.ratio(), 0.0);
    }
}
