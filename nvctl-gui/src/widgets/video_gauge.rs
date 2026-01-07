//! Video codec gauge widget
//!
//! Displays encoder and decoder utilization with dual arcs.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Radians, Rectangle, Renderer, Theme};
use nvctl::domain::performance::{DecoderUtilization, EncoderUtilization};

/// Video codec gauge widget showing encoder and decoder usage
pub struct VideoGauge {
    encoder: Option<EncoderUtilization>,
    decoder: Option<DecoderUtilization>,
}

impl VideoGauge {
    /// Create a new video codec gauge
    pub fn new(encoder: Option<EncoderUtilization>, decoder: Option<DecoderUtilization>) -> Self {
        Self { encoder, decoder }
    }

    /// Get encoder utilization percentage
    fn encoder_percent(&self) -> u8 {
        self.encoder.as_ref().map(|e| e.percent()).unwrap_or(0)
    }

    /// Get decoder utilization percentage
    fn decoder_percent(&self) -> u8 {
        self.decoder.as_ref().map(|d| d.percent()).unwrap_or(0)
    }

    /// Get max utilization for color
    fn max_util(&self) -> u8 {
        self.encoder_percent().max(self.decoder_percent())
    }

    /// Get utilization color
    fn util_color(&self) -> Color {
        let max_util = self.max_util();
        if max_util > 80 {
            colors::ACCENT_RED
        } else if max_util > 50 {
            colors::ACCENT_ORANGE
        } else if max_util > 0 {
            colors::ACCENT_CYAN
        } else {
            colors::TEXT_MUTED
        }
    }
}

impl canvas::Program<Message> for VideoGauge {
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

        let main_color = self.util_color();
        let _bright_color = colors::glossy_highlight(main_color);

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
        // VALUE ARCS - Encoder (outer) and Decoder (inner)
        // ═══════════════════════════════════════════════════════════════════════

        // Encoder arc (outer, slightly offset outward)
        if self.encoder.is_some() {
            let encoder_ratio = (self.encoder_percent() as f32 / 100.0).clamp(0.0, 1.0);
            let encoder_angle = start_angle + sweep * encoder_ratio;
            let encoder_color = colors::ACCENT_PURPLE;

            // Encoder glow
            draw_arc(
                &mut frame,
                center,
                radius + 3.0,
                Radians(start_angle),
                Radians(encoder_angle),
                10.0,
                colors::with_alpha(encoder_color, 0.15),
            );

            // Encoder arc
            draw_arc(
                &mut frame,
                center,
                radius,
                Radians(start_angle),
                Radians(encoder_angle),
                8.0,
                encoder_color,
            );

            // Highlight
            draw_arc(
                &mut frame,
                center,
                radius - 1.0,
                Radians(start_angle),
                Radians(encoder_angle),
                3.0,
                colors::with_alpha(colors::glossy_highlight(encoder_color), 0.5),
            );
        }

        // Decoder arc (inner, slightly offset inward)
        if self.decoder.is_some() {
            let decoder_ratio = (self.decoder_percent() as f32 / 100.0).clamp(0.0, 1.0);
            let decoder_angle = start_angle + sweep * decoder_ratio;
            let decoder_color = colors::ACCENT_GREEN;

            // Decoder arc (smaller radius)
            draw_arc(
                &mut frame,
                center,
                radius - 10.0,
                Radians(start_angle),
                Radians(decoder_angle),
                8.0,
                decoder_color,
            );

            // Highlight
            draw_arc(
                &mut frame,
                center,
                radius - 11.0,
                Radians(start_angle),
                Radians(decoder_angle),
                3.0,
                colors::with_alpha(colors::glossy_highlight(decoder_color), 0.5),
            );
        }

        // ═══════════════════════════════════════════════════════════════════════
        // CENTER TEXT - Encoder and Decoder percentages
        // ═══════════════════════════════════════════════════════════════════════
        if self.encoder.is_some() || self.decoder.is_some() {
            // Encoder percentage
            let enc_text = format!("E: {}%", self.encoder_percent());
            frame.fill_text(Text {
                content: enc_text,
                position: Point::new(center.x, center.y - 8.0),
                color: colors::ACCENT_PURPLE,
                size: 16.0.into(),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Bottom,
                ..Default::default()
            });

            // Decoder percentage
            let dec_text = format!("D: {}%", self.decoder_percent());
            frame.fill_text(Text {
                content: dec_text,
                position: Point::new(center.x, center.y + 8.0),
                color: colors::ACCENT_GREEN,
                size: 16.0.into(),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Top,
                ..Default::default()
            });
        } else {
            // N/A text
            frame.fill_text(Text {
                content: "N/A".to_string(),
                position: Point::new(center.x, center.y),
                color: colors::TEXT_MUTED,
                size: 24.0.into(),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Center,
                ..Default::default()
            });
        }

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
