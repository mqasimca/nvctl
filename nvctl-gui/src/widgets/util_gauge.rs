//! GPU Utilization gauge widget
//!
//! Glossy circular arc gauge showing GPU utilization percentage.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Radians, Rectangle, Renderer, Theme, Vector};
use nvctl::domain::Utilization;

/// GPU Utilization gauge widget with glossy glass effect
pub struct UtilGauge {
    utilization: Utilization,
}

impl UtilGauge {
    /// Create a new utilization gauge
    pub fn new(utilization: Utilization) -> Self {
        Self { utilization }
    }

    /// Get the GPU utilization ratio (0.0 - 1.0)
    fn ratio(&self) -> f32 {
        self.utilization.gpu_percent() as f32 / 100.0
    }
}

impl canvas::Program<Message> for UtilGauge {
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

        // Calculate utilization ratio and angle
        let util_ratio = self.ratio();
        let value_angle = start_angle + sweep * util_ratio;

        // Get gradient color based on utilization (blue -> purple -> magenta)
        let main_color = util_gradient(util_ratio);
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
            200.0_f32.to_radians(),
            340.0_f32.to_radians(),
            3.0,
            colors::GLASS_SHINE,
        );

        // Subtle ring around center
        let center_ring = Path::circle(center, inner_radius);
        frame.stroke(
            &center_ring,
            Stroke::default()
                .with_width(1.5)
                .with_color(colors::with_alpha(main_color, 0.3)),
        );

        // ═══════════════════════════════════════════════════════════════════════
        // BACKGROUND TRACK - Clean glossy track
        // ═══════════════════════════════════════════════════════════════════════
        draw_arc(
            &mut frame,
            center,
            radius,
            start_angle,
            start_angle + sweep,
            12.0,
            colors::BG_ELEVATED,
        );

        // Track highlight (glossy top edge)
        draw_arc(
            &mut frame,
            center,
            radius + 4.0,
            start_angle,
            start_angle + sweep,
            2.0,
            colors::GLASS_HIGHLIGHT,
        );

        // ═══════════════════════════════════════════════════════════════════════
        // VALUE ARC - Colorful glossy fill with gradient
        // ═══════════════════════════════════════════════════════════════════════
        if util_ratio > 0.0 {
            // Outer soft glow
            draw_arc(
                &mut frame,
                center,
                radius,
                start_angle,
                value_angle,
                20.0,
                colors::with_alpha(main_color, 0.15),
            );

            // Main colored arc
            draw_arc(
                &mut frame,
                center,
                radius,
                start_angle,
                value_angle,
                12.0,
                main_color,
            );

            // Glossy shine on top of arc (bright highlight)
            draw_arc(
                &mut frame,
                center,
                radius + 3.0,
                start_angle,
                value_angle,
                3.0,
                colors::with_alpha(bright_color, 0.5),
            );

            // Inner bright edge
            draw_arc(
                &mut frame,
                center,
                radius - 4.0,
                start_angle,
                value_angle,
                2.0,
                colors::with_alpha(colors::TEXT_PRIMARY, 0.15),
            );
        }

        // ═══════════════════════════════════════════════════════════════════════
        // END CAP - Glossy glowing orb
        // ═══════════════════════════════════════════════════════════════════════
        if util_ratio > 0.01 {
            let end_x = center.x + radius * value_angle.cos();
            let end_y = center.y + radius * value_angle.sin();

            // Outer glow halo
            let outer_glow = Path::circle(Point::new(end_x, end_y), 14.0);
            frame.fill(&outer_glow, colors::with_alpha(main_color, 0.2));

            // Mid glow
            let mid_glow = Path::circle(Point::new(end_x, end_y), 10.0);
            frame.fill(&mid_glow, colors::with_alpha(main_color, 0.4));

            // Core orb
            let core_dot = Path::circle(Point::new(end_x, end_y), 7.0);
            frame.fill(&core_dot, main_color);

            // Glossy highlight spot (top-left)
            let highlight = Path::circle(Point::new(end_x - 2.0, end_y - 2.0), 3.0);
            frame.fill(&highlight, colors::with_alpha(bright_color, 0.7));
        }

        // ═══════════════════════════════════════════════════════════════════════
        // UTILIZATION TEXT - Clean, bold centered
        // ═══════════════════════════════════════════════════════════════════════
        let util_text = format!("{}%", self.utilization.gpu_percent());
        frame.fill_text(Text {
            content: util_text,
            position: center + Vector::new(0.0, -4.0),
            color: colors::TEXT_PRIMARY,
            size: 32.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // Label in accent color
        frame.fill_text(Text {
            content: "GPU".to_string(),
            position: center + Vector::new(0.0, 20.0),
            color: colors::with_alpha(main_color, 0.8),
            size: 11.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        vec![frame.into_geometry()]
    }
}

/// Get color gradient for utilization (blue -> purple -> magenta)
fn util_gradient(ratio: f32) -> Color {
    // Blue at low, purple at mid, magenta at high
    if ratio < 0.5 {
        // Blue to purple
        let t = ratio * 2.0;
        colors::lerp(colors::ACCENT_BLUE, colors::ACCENT_PURPLE, t)
    } else {
        // Purple to magenta
        let t = (ratio - 0.5) * 2.0;
        colors::lerp(colors::ACCENT_PURPLE, colors::ACCENT_MAGENTA, t)
    }
}

/// Draw an arc with round caps
fn draw_arc(
    frame: &mut Frame,
    center: Point,
    radius: f32,
    start: f32,
    end: f32,
    width: f32,
    color: Color,
) {
    let arc = Path::new(|builder| {
        builder.arc(canvas::path::Arc {
            center,
            radius,
            start_angle: Radians(start),
            end_angle: Radians(end),
        });
    });

    frame.stroke(
        &arc,
        Stroke::default()
            .with_width(width)
            .with_color(color)
            .with_line_cap(canvas::LineCap::Round),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_util_gauge_ratio() {
        let gauge = UtilGauge::new(Utilization::new(75, 50));
        assert!((gauge.ratio() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_util_gauge_zero() {
        let gauge = UtilGauge::new(Utilization::new(0, 0));
        assert_eq!(gauge.ratio(), 0.0);
    }

    #[test]
    fn test_util_gradient() {
        let low = util_gradient(0.0);
        let high = util_gradient(1.0);
        // Should be different colors
        assert!(low.r != high.r || low.g != high.g || low.b != high.b);
    }
}
