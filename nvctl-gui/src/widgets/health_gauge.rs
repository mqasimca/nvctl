//! GPU health score gauge widget
//!
//! Displays overall GPU health score with color-coded status.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Radians, Rectangle, Renderer, Theme};
use nvctl::health::HealthScore;

/// GPU health gauge widget
#[allow(dead_code)]
pub struct HealthGauge {
    health_score: HealthScore,
}

#[allow(dead_code)]
impl HealthGauge {
    /// Create a new health gauge
    pub fn new(health_score: HealthScore) -> Self {
        Self { health_score }
    }

    /// Get health color based on score
    fn health_color(&self) -> Color {
        match self.health_score.score() {
            90..=100 => colors::ACCENT_GREEN, // Excellent
            75..=89 => colors::ACCENT_CYAN,   // Good
            50..=74 => colors::ACCENT_CYAN,   // Fair
            25..=49 => colors::ACCENT_ORANGE, // Poor
            _ => colors::ACCENT_RED,          // Critical
        }
    }

    /// Get status text
    fn status_text(&self) -> &'static str {
        match self.health_score.score() {
            90..=100 => "Excellent",
            75..=89 => "Good",
            50..=74 => "Fair",
            25..=49 => "Poor",
            _ => "Critical",
        }
    }
}

impl canvas::Program<Message> for HealthGauge {
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
        let center = frame.center();
        let radius = bounds.width.min(bounds.height) / 2.0 - 20.0;

        // Arc configuration: 270° sweep, starting from bottom-left
        let start_angle = 135.0_f32.to_radians();
        let sweep = 270.0_f32.to_radians();

        let main_color = self.health_color();
        let bright_color = colors::glossy_highlight(main_color);

        // Calculate health ratio and angle (0-100 score)
        let health_ratio = (self.health_score.score() as f32 / 100.0).clamp(0.0, 1.0);
        let value_angle = start_angle + sweep * health_ratio;

        // ═══════════════════════════════════════════════════════════════════════
        // GLOSSY CENTER DISK
        // ═══════════════════════════════════════════════════════════════════════
        let inner_radius = radius - 22.0;

        let glass_bg = Path::circle(center, inner_radius);
        frame.fill(&glass_bg, colors::BG_SURFACE);

        // Glass highlight
        draw_arc(
            &mut frame,
            center,
            inner_radius - 2.0,
            Radians(200.0_f32.to_radians()),
            Radians(340.0_f32.to_radians()),
            8.0,
            colors::with_alpha(colors::GLASS_HIGHLIGHT, 0.15),
        );

        // ═══════════════════════════════════════════════════════════════════════
        // BACKGROUND TRACK
        // ═══════════════════════════════════════════════════════════════════════
        draw_arc(
            &mut frame,
            center,
            radius,
            Radians(start_angle),
            Radians(start_angle + sweep),
            12.0,
            colors::with_alpha(colors::BG_ELEVATED, 0.4),
        );

        // ═══════════════════════════════════════════════════════════════════════
        // VALUE ARC - Health score
        // ═══════════════════════════════════════════════════════════════════════
        // Outer glow
        draw_arc(
            &mut frame,
            center,
            radius + 3.0,
            Radians(start_angle),
            Radians(value_angle),
            16.0,
            colors::with_alpha(main_color, 0.15),
        );

        // Main arc
        draw_arc(
            &mut frame,
            center,
            radius,
            Radians(start_angle),
            Radians(value_angle),
            12.0,
            main_color,
        );

        // Glossy highlight
        draw_arc(
            &mut frame,
            center,
            radius - 1.0,
            Radians(start_angle),
            Radians(value_angle),
            4.0,
            colors::with_alpha(bright_color, 0.5),
        );

        // ═══════════════════════════════════════════════════════════════════════
        // CENTER TEXT - Health score and status
        // ═══════════════════════════════════════════════════════════════════════
        // Health score value
        frame.fill_text(Text {
            content: self.health_score.score().to_string(),
            position: Point::new(center.x, center.y - 8.0),
            color: main_color,
            size: 32.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Bottom,
            ..Default::default()
        });

        // Status text
        frame.fill_text(Text {
            content: self.status_text().to_string(),
            position: Point::new(center.x, center.y + 12.0),
            color: colors::TEXT_SECONDARY,
            size: 10.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Top,
            ..Default::default()
        });

        vec![frame.into_geometry()]
    }
}

/// Draw an arc with given parameters
#[allow(dead_code)]
fn draw_arc(
    frame: &mut Frame,
    center: Point,
    radius: f32,
    start: Radians,
    end: Radians,
    width: f32,
    color: Color,
) {
    let arc = Path::new(|p| {
        p.arc(canvas::path::Arc {
            center,
            radius,
            start_angle: start,
            end_angle: end,
        });
    });

    frame.stroke(
        &arc,
        Stroke {
            style: color.into(),
            width,
            line_cap: iced::widget::canvas::LineCap::Round,
            ..Default::default()
        },
    );
}
