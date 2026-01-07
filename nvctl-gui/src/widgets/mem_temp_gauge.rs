//! Memory temperature gauge widget
//!
//! Displays GDDR memory temperature separate from GPU die temperature.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Radians, Rectangle, Renderer, Theme};
use nvctl::domain::Temperature;

/// Memory temperature gauge widget
pub struct MemTempGauge {
    temperature: Option<Temperature>,
}

impl MemTempGauge {
    /// Create a new memory temperature gauge
    pub fn new(temperature: Option<Temperature>) -> Self {
        Self { temperature }
    }

    /// Get the maximum temperature for the gauge scale
    fn max_temp(&self) -> i32 {
        110 // GDDR6X safe maximum
    }

    /// Get temperature color based on GDDR6X thermal limits
    fn temp_color(&self) -> Color {
        if let Some(temp) = self.temperature {
            let celsius = temp.as_celsius();
            if celsius < 95 {
                colors::ACCENT_GREEN
            } else if celsius < 100 {
                colors::ACCENT_CYAN
            } else if celsius < 105 {
                colors::ACCENT_ORANGE
            } else {
                colors::ACCENT_RED
            }
        } else {
            colors::TEXT_MUTED
        }
    }
}

impl canvas::Program<Message> for MemTempGauge {
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

        let main_color = self.temp_color();
        let bright_color = colors::glossy_highlight(main_color);

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
        // VALUE ARC - Memory temperature
        // ═══════════════════════════════════════════════════════════════════════
        if let Some(temp) = self.temperature {
            let temp_ratio = (temp.as_celsius() as f32 / self.max_temp() as f32).clamp(0.0, 1.0);
            let value_angle = start_angle + sweep * temp_ratio;

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
        }

        // ═══════════════════════════════════════════════════════════════════════
        // CENTER TEXT - Temperature value
        // ═══════════════════════════════════════════════════════════════════════
        let (value_text, unit_text) = if let Some(temp) = self.temperature {
            (temp.as_celsius().to_string(), "°C".to_string())
        } else {
            ("N/A".to_string(), "".to_string())
        };

        // Temperature value
        frame.fill_text(Text {
            content: value_text,
            position: Point::new(center.x, center.y - 8.0),
            color: main_color,
            size: 32.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Bottom,
            ..Default::default()
        });

        // Unit
        frame.fill_text(Text {
            content: unit_text,
            position: Point::new(center.x, center.y + 12.0),
            color: colors::TEXT_SECONDARY,
            size: 11.0.into(),
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
