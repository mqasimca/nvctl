//! ECC memory health gauge widget
//!
//! Displays ECC error status and health indication.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Radians, Rectangle, Renderer, Theme};
use nvctl::domain::memory::{EccErrors, EccHealthStatus};

/// ECC health gauge widget showing error status
pub struct EccGauge {
    ecc_errors: Option<EccErrors>,
    uptime_seconds: u64,
}

impl EccGauge {
    /// Create a new ECC gauge
    pub fn new(ecc_errors: Option<EccErrors>, uptime_seconds: u64) -> Self {
        Self {
            ecc_errors,
            uptime_seconds,
        }
    }

    /// Get health status
    fn health_status(&self) -> EccHealthStatus {
        self.ecc_errors
            .as_ref()
            .map(|e| e.health_status(self.uptime_seconds))
            .unwrap_or(EccHealthStatus::Healthy)
    }

    /// Get health color
    fn health_color(&self) -> Color {
        match self.health_status() {
            EccHealthStatus::Healthy => colors::ACCENT_GREEN,
            EccHealthStatus::Fair => colors::ACCENT_CYAN,
            EccHealthStatus::Warning => colors::ACCENT_ORANGE,
            EccHealthStatus::Critical => colors::ACCENT_RED,
        }
    }

    /// Get total error count
    fn total_errors(&self) -> u64 {
        self.ecc_errors
            .as_ref()
            .map(|e| e.correctable_current + e.uncorrectable_current)
            .unwrap_or(0)
    }
}

impl canvas::Program<Message> for EccGauge {
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

        // ═══════════════════════════════════════════════════════════════════════
        // GLOSSY CENTER DISK - Glass effect background
        // ═══════════════════════════════════════════════════════════════════════
        let inner_radius = radius - 22.0;

        // Base glass circle
        let glass_bg = Path::circle(center, inner_radius);
        frame.fill(&glass_bg, colors::BG_SURFACE);

        // Glass highlight (top arc shine)
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
        // BACKGROUND TRACK - Dim arc outline
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
        // VALUE ARC - Health status arc (full sweep for healthy, partial for errors)
        // ═══════════════════════════════════════════════════════════════════════
        let value_ratio = if self.ecc_errors.is_none() {
            0.0 // No ECC support
        } else {
            match self.health_status() {
                EccHealthStatus::Healthy => 1.0,
                EccHealthStatus::Fair => 0.75,
                EccHealthStatus::Warning => 0.5,
                EccHealthStatus::Critical => 0.25,
            }
        };
        let value_angle = start_angle + sweep * value_ratio;

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

        // Glossy highlight on arc
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
        // CENTER TEXT - Error count and health status
        // ═══════════════════════════════════════════════════════════════════════
        let (value_text, unit_text) = if self.ecc_errors.is_none() {
            ("N/A".to_string(), "".to_string())
        } else {
            (
                self.total_errors().to_string(),
                format!("{:?}", self.health_status()),
            )
        };

        // Error count
        frame.fill_text(Text {
            content: value_text,
            position: Point::new(center.x, center.y - 8.0),
            color: main_color,
            size: 32.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Bottom,
            ..Default::default()
        });

        // Health status
        frame.fill_text(Text {
            content: unit_text,
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
