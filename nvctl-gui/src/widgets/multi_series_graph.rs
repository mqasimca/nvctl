//! Multi-series time graph widget
//!
//! Premium line graph showing multiple metrics with gradient fills and legends.

use crate::message::Message;
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};
use std::collections::VecDeque;

/// A single data series with its styling
pub struct DataSeries<'a> {
    /// Data points
    pub data: &'a VecDeque<f32>,
    /// Series name for legend
    pub name: &'a str,
    /// Unit suffix (e.g., "°C", "%", "W")
    pub unit: &'a str,
    /// Line color
    pub color: Color,
    /// Min value for normalization
    pub min: f32,
    /// Max value for normalization
    pub max: f32,
}

impl<'a> DataSeries<'a> {
    /// Create a new data series
    pub fn new(data: &'a VecDeque<f32>, name: &'a str, unit: &'a str, color: Color) -> Self {
        Self {
            data,
            name,
            unit,
            color,
            min: 0.0,
            max: 100.0,
        }
    }

    /// Set the value range for normalization
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// Normalize a value to 0.0-1.0 range
    fn normalize(&self, value: f32) -> f32 {
        let range = self.max - self.min;
        if range <= 0.0 {
            return 0.5;
        }
        ((value - self.min) / range).clamp(0.0, 1.0)
    }
}

/// Multi-series time graph widget for displaying multiple metrics
pub struct MultiSeriesGraph<'a> {
    series: Vec<DataSeries<'a>>,
    title: &'a str,
}

impl<'a> MultiSeriesGraph<'a> {
    /// Create a new multi-series graph
    pub fn new(title: &'a str) -> Self {
        Self {
            series: Vec::new(),
            title,
        }
    }

    /// Add a data series
    pub fn add_series(mut self, series: DataSeries<'a>) -> Self {
        self.series.push(series);
        self
    }
}

impl canvas::Program<Message> for MultiSeriesGraph<'_> {
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
        let margin_left = 15.0;
        let margin_right = 15.0;
        let margin_top = 50.0; // Extra space for title and legend
        let margin_bottom = 25.0;

        let graph_width = bounds.width - margin_left - margin_right;
        let graph_height = bounds.height - margin_top - margin_bottom;

        // ═══════════════════════════════════════════════════════════════════════
        // TITLE
        // ═══════════════════════════════════════════════════════════════════════
        frame.fill_text(Text {
            content: self.title.to_string(),
            position: Point::new(margin_left, 12.0),
            color: colors::TEXT_PRIMARY,
            size: 14.0.into(),
            horizontal_alignment: Horizontal::Left,
            vertical_alignment: Vertical::Center,
            ..Text::default()
        });

        // ═══════════════════════════════════════════════════════════════════════
        // LEGEND - Show all series with current values (evenly spaced)
        // ═══════════════════════════════════════════════════════════════════════
        let legend_y = 32.0;
        let dot_radius = 5.0;
        let series_count = self.series.len().max(1) as f32;
        let legend_width = bounds.width - margin_left - margin_right;
        let item_width = legend_width / series_count;

        for (i, series) in self.series.iter().enumerate() {
            let item_x = margin_left + i as f32 * item_width;

            // Color dot with glow
            let glow = Path::circle(Point::new(item_x + dot_radius, legend_y), dot_radius + 3.0);
            frame.fill(&glow, colors::with_alpha(series.color, 0.2));

            let dot = Path::circle(Point::new(item_x + dot_radius, legend_y), dot_radius);
            frame.fill(&dot, series.color);

            // Series name and current value
            let current = series.data.back().copied().unwrap_or(0.0);
            let label = format!("{}: {:.0}{}", series.name, current, series.unit);
            let label_x = item_x + dot_radius * 2.0 + 8.0;

            frame.fill_text(Text {
                content: label,
                position: Point::new(label_x, legend_y),
                color: series.color,
                size: 12.0.into(),
                horizontal_alignment: Horizontal::Left,
                vertical_alignment: Vertical::Center,
                ..Text::default()
            });
        }

        // ═══════════════════════════════════════════════════════════════════════
        // GRAPH BACKGROUND
        // ═══════════════════════════════════════════════════════════════════════
        let bg_rect = Path::rectangle(
            Point::new(margin_left, margin_top),
            iced::Size::new(graph_width, graph_height),
        );
        frame.fill(&bg_rect, colors::BG_ELEVATED);

        // ═══════════════════════════════════════════════════════════════════════
        // GRID LINES - Subtle horizontal lines
        // ═══════════════════════════════════════════════════════════════════════
        let grid_lines = 4;
        for i in 0..=grid_lines {
            let ratio = i as f32 / grid_lines as f32;
            let y = margin_top + graph_height * (1.0 - ratio);

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
        }

        // ═══════════════════════════════════════════════════════════════════════
        // DATA SERIES - Draw each series
        // ═══════════════════════════════════════════════════════════════════════
        for series in &self.series {
            if series.data.len() >= 2 {
                let points = series.data.len();
                let x_step = graph_width / (points - 1).max(1) as f32;

                // Gradient fill under the line
                let fill_path = Path::new(|builder| {
                    builder.move_to(Point::new(margin_left, margin_top + graph_height));

                    for (i, &value) in series.data.iter().enumerate() {
                        let x = margin_left + i as f32 * x_step;
                        let normalized = series.normalize(value);
                        let y = margin_top + graph_height * (1.0 - normalized);
                        builder.line_to(Point::new(x, y));
                    }

                    let last_x = margin_left + (points - 1) as f32 * x_step;
                    builder.line_to(Point::new(last_x, margin_top + graph_height));
                    builder.close();
                });

                frame.fill(&fill_path, colors::with_alpha(series.color, 0.06));

                // Main data line
                let line = Path::new(|builder| {
                    for (i, &value) in series.data.iter().enumerate() {
                        let x = margin_left + i as f32 * x_step;
                        let normalized = series.normalize(value);
                        let y = margin_top + graph_height * (1.0 - normalized);

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
                        .with_width(4.0)
                        .with_color(colors::with_alpha(series.color, 0.15))
                        .with_line_cap(canvas::LineCap::Round)
                        .with_line_join(canvas::LineJoin::Round),
                );

                // Core line
                frame.stroke(
                    &line,
                    Stroke::default()
                        .with_width(2.0)
                        .with_color(series.color)
                        .with_line_cap(canvas::LineCap::Round)
                        .with_line_join(canvas::LineJoin::Round),
                );

                // Current value marker (glowing dot)
                if let Some(&current) = series.data.back() {
                    let x = margin_left + (points - 1) as f32 * x_step;
                    let normalized = series.normalize(current);
                    let y = margin_top + graph_height * (1.0 - normalized);

                    // Glow
                    let glow = Path::circle(Point::new(x, y), 6.0);
                    frame.fill(&glow, colors::with_alpha(series.color, 0.3));

                    // Core dot
                    let dot = Path::circle(Point::new(x, y), 4.0);
                    frame.fill(&dot, series.color);

                    // Highlight
                    let highlight = Path::circle(Point::new(x - 1.0, y - 1.0), 1.5);
                    frame.fill(&highlight, colors::with_alpha(colors::TEXT_PRIMARY, 0.5));
                }
            }
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
        // BORDER
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_series_creation() {
        let data = VecDeque::from([50.0, 55.0, 60.0]);
        let series = DataSeries::new(&data, "Test", "%", colors::ACCENT_CYAN);
        assert_eq!(series.name, "Test");
        assert_eq!(series.unit, "%");
    }

    #[test]
    fn test_data_series_normalization() {
        let data = VecDeque::from([50.0]);
        let series = DataSeries::new(&data, "Test", "%", colors::ACCENT_CYAN).range(0.0, 100.0);

        assert!((series.normalize(0.0) - 0.0).abs() < 0.001);
        assert!((series.normalize(50.0) - 0.5).abs() < 0.001);
        assert!((series.normalize(100.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_multi_series_graph_creation() {
        let data = VecDeque::from([50.0]);
        let series = DataSeries::new(&data, "Test", "%", colors::ACCENT_CYAN);
        let graph = MultiSeriesGraph::new("Performance").add_series(series);
        assert_eq!(graph.title, "Performance");
        assert_eq!(graph.series.len(), 1);
    }
}
