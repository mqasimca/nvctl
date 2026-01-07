//! PCIe bandwidth gauge widget
//!
//! Displays PCIe throughput and bandwidth efficiency.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Radians, Rectangle, Renderer, Theme};
use nvctl::domain::pcie::PcieMetrics;

/// PCIe bandwidth gauge widget
pub struct PcieGauge {
    metrics: PcieMetrics,
}

impl PcieGauge {
    /// Create a new PCIe gauge
    pub fn new(metrics: PcieMetrics) -> Self {
        Self { metrics }
    }

    /// Get bandwidth utilization percentage (0-100)
    fn bandwidth_utilization(&self) -> f32 {
        let max_bandwidth_gbps = self.metrics.link_status.max_bandwidth_gbps();
        let current_gbps = (self.metrics.throughput.tx_bytes_per_sec()
            + self.metrics.throughput.rx_bytes_per_sec()) as f64
            / 1024.0
            / 1024.0
            / 1024.0;

        if max_bandwidth_gbps > 0.0 {
            ((current_gbps / max_bandwidth_gbps) * 100.0).min(100.0) as f32
        } else {
            0.0
        }
    }

    /// Get bandwidth efficiency color
    fn efficiency_color(&self) -> Color {
        let efficiency = self.metrics.link_status.bandwidth_efficiency_percent();
        if efficiency < 50.0 {
            colors::ACCENT_ORANGE // Warning: not using full link capability
        } else if efficiency < 80.0 {
            colors::ACCENT_CYAN
        } else {
            colors::ACCENT_GREEN // Good link status
        }
    }
}

impl canvas::Program<Message> for PcieGauge {
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

        let main_color = self.efficiency_color();
        let bright_color = colors::glossy_highlight(main_color);

        // Calculate utilization ratio and angle
        let util_ratio = (self.bandwidth_utilization() / 100.0).clamp(0.0, 1.0);
        let value_angle = start_angle + sweep * util_ratio;

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
        // VALUE ARC - Bandwidth utilization
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
        // CENTER TEXT - Generation and throughput
        // ═══════════════════════════════════════════════════════════════════════
        let gen_text = format!(
            "{} {}",
            self.metrics.link_status.current_generation, self.metrics.link_status.current_width
        );

        // PCIe generation/width
        frame.fill_text(Text {
            content: gen_text,
            position: Point::new(center.x, center.y - 8.0),
            color: main_color,
            size: 24.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Bottom,
            ..Default::default()
        });

        // Utilization percentage
        let util_text = format!("{:.0}%", self.bandwidth_utilization());
        frame.fill_text(Text {
            content: util_text,
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
