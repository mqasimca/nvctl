//! Time series graph widget
//!
//! Premium line graph with gradient fills and glowing data points.

#![allow(dead_code)]

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};
use std::collections::VecDeque;

/// Time series graph widget for displaying historical metrics
pub struct TimeSeriesGraph<'a> {
    data: &'a VecDeque<f32>,
    label: &'a str,
    unit: &'a str,
    color: Color,
    min_value: f32,
    max_value: f32,
    grid_lines: u8,
}

impl<'a> TimeSeriesGraph<'a> {
    /// Create a new time series graph
    pub fn new(data: &'a VecDeque<f32>, label: &'a str, unit: &'a str) -> Self {
        let (min, max) = if data.is_empty() {
            (0.0, 100.0)
        } else {
            let min = data.iter().copied().fold(f32::INFINITY, f32::min);
            let max = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let range = (max - min).max(1.0);
            (min - range * 0.1, max + range * 0.1)
        };

        Self {
            data,
            label,
            unit,
            color: colors::ACCENT_CYAN,
            min_value: min.max(0.0),
            max_value: max,
            grid_lines: 4,
        }
    }

    /// Set the line color
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the value range
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min_value = min;
        self.max_value = max;
        self
    }

    /// Set the number of horizontal grid lines
    pub fn grid_lines(mut self, count: u8) -> Self {
        self.grid_lines = count;
        self
    }
}

impl canvas::Program<Message> for TimeSeriesGraph<'_> {
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

        // Define drawing area with margins
        let margin_left = 45.0;
        let margin_right = 15.0;
        let margin_top = 30.0;
        let margin_bottom = 25.0;

        let graph_width = bounds.width - margin_left - margin_right;
        let graph_height = bounds.height - margin_top - margin_bottom;

        // ═══════════════════════════════════════════════════════════════════════
        // HEADER - Label and current value
        // ═══════════════════════════════════════════════════════════════════════
        frame.fill_text(Text {
            content: self.label.to_string(),
            position: Point::new(margin_left, 12.0),
            color: colors::TEXT_PRIMARY,
            size: 14.0.into(),
            horizontal_alignment: Horizontal::Left,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        if let Some(&current) = self.data.back() {
            let value_text = format!("{:.0}{}", current, self.unit);
            frame.fill_text(Text {
                content: value_text,
                position: Point::new(bounds.width - margin_right, 12.0),
                color: self.color,
                size: 14.0.into(),
                horizontal_alignment: Horizontal::Right,
                vertical_alignment: Vertical::Center,
                ..Text::default()
            });
        }

        // ═══════════════════════════════════════════════════════════════════════
        // GRAPH BACKGROUND - Dark area
        // ═══════════════════════════════════════════════════════════════════════
        let bg_rect = Path::rectangle(
            Point::new(margin_left, margin_top),
            iced::Size::new(graph_width, graph_height),
        );
        frame.fill(&bg_rect, colors::BG_ELEVATED);

        // ═══════════════════════════════════════════════════════════════════════
        // GRID LINES - Subtle horizontal lines with labels
        // ═══════════════════════════════════════════════════════════════════════
        let value_range = self.max_value - self.min_value;
        for i in 0..=self.grid_lines {
            let ratio = i as f32 / self.grid_lines as f32;
            let y = margin_top + graph_height * (1.0 - ratio);
            let value = self.min_value + value_range * ratio;

            // Grid line
            let line = Path::line(
                Point::new(margin_left, y),
                Point::new(margin_left + graph_width, y),
            );
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(1.0)
                    .with_color(colors::with_alpha(colors::BG_OVERLAY, 0.5)),
            );

            // Y-axis label
            frame.fill_text(Text {
                content: format!("{:.0}", value),
                position: Point::new(margin_left - 8.0, y),
                color: colors::TEXT_MUTED,
                size: 10.0.into(),
                horizontal_alignment: Horizontal::Right,
                vertical_alignment: Vertical::Center,
                ..Text::default()
            });
        }

        // ═══════════════════════════════════════════════════════════════════════
        // DATA VISUALIZATION
        // ═══════════════════════════════════════════════════════════════════════
        if self.data.len() >= 2 {
            let points = self.data.len();
            let x_step = graph_width / (points - 1).max(1) as f32;

            // Gradient fill under the line
            let fill_path = Path::new(|builder| {
                builder.move_to(Point::new(margin_left, margin_top + graph_height));

                for (i, &value) in self.data.iter().enumerate() {
                    let x = margin_left + i as f32 * x_step;
                    let normalized = (value - self.min_value) / value_range;
                    let y = margin_top + graph_height * (1.0 - normalized.clamp(0.0, 1.0));
                    builder.line_to(Point::new(x, y));
                }

                let last_x = margin_left + (points - 1) as f32 * x_step;
                builder.line_to(Point::new(last_x, margin_top + graph_height));
                builder.close();
            });

            // Multiple fill layers for gradient effect
            frame.fill(&fill_path, colors::with_alpha(self.color, 0.08));

            // Inner fill (brighter at bottom)
            let inner_fill = Path::new(|builder| {
                let threshold_y = margin_top + graph_height * 0.7;
                builder.move_to(Point::new(margin_left, margin_top + graph_height));

                for (i, &value) in self.data.iter().enumerate() {
                    let x = margin_left + i as f32 * x_step;
                    let normalized = (value - self.min_value) / value_range;
                    let y = margin_top + graph_height * (1.0 - normalized.clamp(0.0, 1.0));
                    let clamped_y = y.max(threshold_y);
                    builder.line_to(Point::new(x, clamped_y));
                }

                let last_x = margin_left + (points - 1) as f32 * x_step;
                builder.line_to(Point::new(last_x, margin_top + graph_height));
                builder.close();
            });
            frame.fill(&inner_fill, colors::with_alpha(self.color, 0.15));

            // Main data line - glow effect
            let line = Path::new(|builder| {
                for (i, &value) in self.data.iter().enumerate() {
                    let x = margin_left + i as f32 * x_step;
                    let normalized = (value - self.min_value) / value_range;
                    let y = margin_top + graph_height * (1.0 - normalized.clamp(0.0, 1.0));

                    if i == 0 {
                        builder.move_to(Point::new(x, y));
                    } else {
                        builder.line_to(Point::new(x, y));
                    }
                }
            });

            // Outer glow
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(6.0)
                    .with_color(colors::with_alpha(self.color, 0.15))
                    .with_line_cap(canvas::LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round),
            );

            // Mid glow
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(4.0)
                    .with_color(colors::with_alpha(self.color, 0.3))
                    .with_line_cap(canvas::LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round),
            );

            // Core line
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(2.0)
                    .with_color(self.color)
                    .with_line_cap(canvas::LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round),
            );

            // ═══════════════════════════════════════════════════════════════════════
            // CURRENT VALUE MARKER - Glowing dot at the end
            // ═══════════════════════════════════════════════════════════════════════
            if let Some(&current) = self.data.back() {
                let x = margin_left + (points - 1) as f32 * x_step;
                let normalized = (current - self.min_value) / value_range;
                let y = margin_top + graph_height * (1.0 - normalized.clamp(0.0, 1.0));

                // Outer glow
                let glow_outer = Path::circle(Point::new(x, y), 10.0);
                frame.fill(&glow_outer, colors::with_alpha(self.color, 0.15));

                // Mid glow
                let glow_mid = Path::circle(Point::new(x, y), 7.0);
                frame.fill(&glow_mid, colors::with_alpha(self.color, 0.3));

                // Core dot
                let dot = Path::circle(Point::new(x, y), 5.0);
                frame.fill(&dot, self.color);

                // Highlight
                let highlight = Path::circle(Point::new(x - 1.5, y - 1.5), 2.0);
                frame.fill(&highlight, colors::with_alpha(colors::TEXT_PRIMARY, 0.5));

                // Outer ring (pulse effect simulation)
                let ring = Path::circle(Point::new(x, y), 8.0);
                frame.stroke(
                    &ring,
                    Stroke::default()
                        .with_width(1.5)
                        .with_color(colors::with_alpha(self.color, 0.4)),
                );
            }
        } else {
            // No data message
            frame.fill_text(Text {
                content: "No data".to_string(),
                position: Point::new(
                    margin_left + graph_width / 2.0,
                    margin_top + graph_height / 2.0,
                ),
                color: colors::TEXT_MUTED,
                size: 12.0.into(),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Center,
                ..Text::default()
            });
        }

        // ═══════════════════════════════════════════════════════════════════════
        // TIME AXIS LABELS
        // ═══════════════════════════════════════════════════════════════════════
        frame.fill_text(Text {
            content: "5 min".to_string(),
            position: Point::new(margin_left, bounds.height - 8.0),
            color: colors::TEXT_MUTED,
            size: 9.0.into(),
            horizontal_alignment: Horizontal::Left,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        frame.fill_text(Text {
            content: "now".to_string(),
            position: Point::new(bounds.width - margin_right, bounds.height - 8.0),
            color: colors::TEXT_MUTED,
            size: 9.0.into(),
            horizontal_alignment: Horizontal::Right,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // ═══════════════════════════════════════════════════════════════════════
        // BORDER - Subtle frame
        // ═══════════════════════════════════════════════════════════════════════
        let border = Path::rectangle(
            Point::new(margin_left, margin_top),
            iced::Size::new(graph_width, graph_height),
        );
        frame.stroke(
            &border,
            Stroke::default()
                .with_width(1.0)
                .with_color(colors::BG_OVERLAY),
        );

        vec![frame.into_geometry()]
    }
}

/// Helper to create a temperature graph with appropriate coloring
pub fn temp_graph<'a>(data: &'a VecDeque<f32>, current_temp: i32) -> TimeSeriesGraph<'a> {
    TimeSeriesGraph::new(data, "Temperature", "°C")
        .color(colors::temp_color(current_temp))
        .range(0.0, 100.0)
}

/// Helper to create a power usage graph
pub fn power_graph<'a>(data: &'a VecDeque<f32>, max_watts: f32) -> TimeSeriesGraph<'a> {
    TimeSeriesGraph::new(data, "Power", "W")
        .color(colors::ACCENT_GREEN)
        .range(0.0, max_watts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_series_graph_creation() {
        let data = VecDeque::from([50.0, 55.0, 60.0, 58.0, 62.0]);
        let graph = TimeSeriesGraph::new(&data, "Test", "units");
        assert_eq!(graph.label, "Test");
        assert_eq!(graph.unit, "units");
    }

    #[test]
    fn test_time_series_empty_data() {
        let data = VecDeque::new();
        let graph = TimeSeriesGraph::new(&data, "Empty", "");
        assert_eq!(graph.min_value, 0.0);
        assert_eq!(graph.max_value, 100.0);
    }

    #[test]
    fn test_time_series_range_override() {
        let data = VecDeque::from([50.0, 55.0]);
        let graph = TimeSeriesGraph::new(&data, "Test", "").range(0.0, 200.0);
        assert_eq!(graph.min_value, 0.0);
        assert_eq!(graph.max_value, 200.0);
    }

    #[test]
    fn test_temp_graph_helper() {
        let data = VecDeque::from([65.0, 70.0]);
        let graph = temp_graph(&data, 70);
        assert_eq!(graph.label, "Temperature");
        assert_eq!(graph.unit, "°C");
    }

    #[test]
    fn test_power_graph_helper() {
        let data = VecDeque::from([200.0, 250.0]);
        let graph = power_graph(&data, 400.0);
        assert_eq!(graph.label, "Power");
        assert_eq!(graph.unit, "W");
        assert_eq!(graph.max_value, 400.0);
    }
}
