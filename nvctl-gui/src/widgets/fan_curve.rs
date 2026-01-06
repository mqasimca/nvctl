//! Fan curve editor widget
//!
//! Interactive canvas widget for editing fan curve points.

use crate::message::{FanControlMessage, Message};
use crate::theme::colors;

use iced::alignment::{Horizontal, Vertical};
use iced::mouse;
use iced::widget::canvas::{self, event, Event, Frame, Geometry, Path, Stroke, Text};
use iced::{Point, Rectangle, Renderer, Theme};
use nvctl::domain::FanCurve;

/// Radius for hit detection on points
const POINT_HIT_RADIUS: f32 = 15.0;
/// Visual radius for points
const POINT_RADIUS: f32 = 10.0;
/// Inner circle radius
const POINT_INNER_RADIUS: f32 = 6.0;

/// Fan curve editor widget
pub struct FanCurveEditor {
    curve: FanCurve,
    current_temp: Option<i32>,
}

/// State for the fan curve editor (tracks interaction)
#[derive(Debug, Clone, Default)]
pub struct EditorState {
    /// Index of point being dragged
    dragging: Option<usize>,
    /// Index of point being hovered
    hovered: Option<usize>,
    /// Last click time for double-click detection
    last_click: Option<std::time::Instant>,
    /// Last click position for double-click detection
    last_click_pos: Option<Point>,
}

impl FanCurveEditor {
    /// Create a new fan curve editor
    pub fn new(curve: FanCurve) -> Self {
        Self {
            curve,
            current_temp: None,
        }
    }

    /// Set the current temperature indicator
    pub fn with_current_temp(mut self, temp: i32) -> Self {
        self.current_temp = Some(temp);
        self
    }

    /// Get the temperature range for the editor
    fn temp_range() -> (i32, i32) {
        (20, 100)
    }

    /// Get the speed range
    fn speed_range() -> (u8, u8) {
        (0, 100)
    }

    /// Left padding for Y-axis labels
    const PADDING_LEFT: f32 = 50.0;
    /// Right padding
    const PADDING_RIGHT: f32 = 15.0;
    /// Top padding
    const PADDING_TOP: f32 = 20.0;
    /// Bottom padding for X-axis labels
    const PADDING_BOTTOM: f32 = 35.0;

    /// Convert temperature to X coordinate
    fn temp_to_x(temp: i32, bounds: &Rectangle) -> f32 {
        let (min_temp, max_temp) = Self::temp_range();
        let width = bounds.width - Self::PADDING_LEFT - Self::PADDING_RIGHT;
        let ratio = (temp - min_temp) as f32 / (max_temp - min_temp) as f32;
        Self::PADDING_LEFT + width * ratio
    }

    /// Convert speed to Y coordinate
    fn speed_to_y(speed: u8, bounds: &Rectangle) -> f32 {
        let (min_speed, max_speed) = Self::speed_range();
        let height = bounds.height - Self::PADDING_TOP - Self::PADDING_BOTTOM;
        let ratio = (speed - min_speed) as f32 / (max_speed - min_speed) as f32;
        // Y is inverted (0 at top)
        bounds.height - Self::PADDING_BOTTOM - height * ratio
    }

    /// Convert X coordinate to temperature
    fn x_to_temp(x: f32, bounds: &Rectangle) -> i32 {
        let (min_temp, max_temp) = Self::temp_range();
        let width = bounds.width - Self::PADDING_LEFT - Self::PADDING_RIGHT;
        let ratio = ((x - Self::PADDING_LEFT) / width).clamp(0.0, 1.0);
        min_temp + ((max_temp - min_temp) as f32 * ratio) as i32
    }

    /// Convert Y coordinate to speed
    fn y_to_speed(y: f32, bounds: &Rectangle) -> u8 {
        let (min_speed, max_speed) = Self::speed_range();
        let height = bounds.height - Self::PADDING_TOP - Self::PADDING_BOTTOM;
        // Y is inverted
        let ratio = ((bounds.height - Self::PADDING_BOTTOM - y) / height).clamp(0.0, 1.0);
        min_speed + ((max_speed - min_speed) as f32 * ratio) as u8
    }

    /// Find which point (if any) is at the given position
    fn point_at_position(&self, position: Point, bounds: &Rectangle) -> Option<usize> {
        let points = self.curve.points();
        for (i, point) in points.iter().enumerate() {
            let px = Self::temp_to_x(point.temperature, bounds);
            let py = Self::speed_to_y(point.speed.as_percentage(), bounds);
            let dx = position.x - px;
            let dy = position.y - py;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance <= POINT_HIT_RADIUS {
                return Some(i);
            }
        }
        None
    }

    /// Check if position is within the graph area
    fn is_in_graph_area(position: Point, bounds: &Rectangle) -> bool {
        position.x >= Self::PADDING_LEFT
            && position.x <= bounds.width - Self::PADDING_RIGHT
            && position.y >= Self::PADDING_TOP
            && position.y <= bounds.height - Self::PADDING_BOTTOM
    }
}

/// Double-click threshold in milliseconds
const DOUBLE_CLICK_MS: u128 = 400;
/// Double-click distance threshold
const DOUBLE_CLICK_DISTANCE: f32 = 10.0;

#[allow(clippy::single_match)]
impl canvas::Program<Message> for FanCurveEditor {
    type State = EditorState;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<Message>) {
        let cursor_position = match cursor.position_in(bounds) {
            Some(pos) => pos,
            None => {
                // Cursor left the canvas, clear hover state
                if state.hovered.is_some() && state.dragging.is_none() {
                    state.hovered = None;
                }
                return (event::Status::Ignored, None);
            }
        };

        match event {
            Event::Mouse(mouse_event) => match mouse_event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    let now = std::time::Instant::now();

                    // Check if clicking on a point
                    if let Some(point_idx) = self.point_at_position(cursor_position, &bounds) {
                        state.dragging = Some(point_idx);
                        state.hovered = Some(point_idx);
                        state.last_click = Some(now);
                        state.last_click_pos = Some(cursor_position);
                        return (event::Status::Captured, None);
                    }

                    // Check for double-click on empty area to add a point
                    if Self::is_in_graph_area(cursor_position, &bounds) {
                        let is_double_click = if let (Some(last_time), Some(last_pos)) =
                            (state.last_click, state.last_click_pos)
                        {
                            let elapsed = now.duration_since(last_time).as_millis();
                            let dx = cursor_position.x - last_pos.x;
                            let dy = cursor_position.y - last_pos.y;
                            let distance = (dx * dx + dy * dy).sqrt();
                            elapsed < DOUBLE_CLICK_MS && distance < DOUBLE_CLICK_DISTANCE
                        } else {
                            false
                        };

                        if is_double_click {
                            // Double-click: add new point
                            let temp = Self::x_to_temp(cursor_position.x, &bounds);
                            let speed = Self::y_to_speed(cursor_position.y, &bounds);

                            state.last_click = None;
                            state.last_click_pos = None;

                            let message = Message::FanControl(FanControlMessage::CurvePointAdded(
                                temp, speed,
                            ));
                            return (event::Status::Captured, Some(message));
                        } else {
                            // Single click: record for potential double-click
                            state.last_click = Some(now);
                            state.last_click_pos = Some(cursor_position);
                        }
                    }
                }
                mouse::Event::ButtonPressed(mouse::Button::Right) => {
                    // Right-click on a point to remove it
                    if let Some(point_idx) = self.point_at_position(cursor_position, &bounds) {
                        // Don't allow removing if only one point left
                        if self.curve.points().len() > 1 {
                            let message = Message::FanControl(
                                FanControlMessage::CurvePointRemoved(point_idx),
                            );
                            return (event::Status::Captured, Some(message));
                        }
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => {
                    if let Some(point_idx) = state.dragging.take() {
                        // Calculate new position and emit message
                        let temp = Self::x_to_temp(cursor_position.x, &bounds);
                        let speed = Self::y_to_speed(cursor_position.y, &bounds);

                        let message = Message::FanControl(FanControlMessage::CurvePointMoved {
                            index: point_idx,
                            temp,
                            speed,
                        });
                        return (event::Status::Captured, Some(message));
                    }
                }
                mouse::Event::CursorMoved { .. } => {
                    if state.dragging.is_some() {
                        // While dragging, continuously update position
                        if let Some(point_idx) = state.dragging {
                            let temp = Self::x_to_temp(cursor_position.x, &bounds);
                            let speed = Self::y_to_speed(cursor_position.y, &bounds);

                            let message = Message::FanControl(FanControlMessage::CurvePointMoved {
                                index: point_idx,
                                temp,
                                speed,
                            });
                            return (event::Status::Captured, Some(message));
                        }
                    } else {
                        // Update hover state
                        let new_hovered = self.point_at_position(cursor_position, &bounds);
                        if new_hovered != state.hovered {
                            state.hovered = new_hovered;
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }

        (event::Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        let graph_width = bounds.width - Self::PADDING_LEFT - Self::PADDING_RIGHT;
        let graph_height = bounds.height - Self::PADDING_TOP - Self::PADDING_BOTTOM;

        // Draw grid background
        let bg = Path::rectangle(
            Point::new(Self::PADDING_LEFT, Self::PADDING_TOP),
            iced::Size::new(graph_width, graph_height),
        );
        frame.fill(&bg, colors::BG_ELEVATED);

        // Draw grid lines (temperature)
        let (min_temp, max_temp) = Self::temp_range();
        for temp in (min_temp..=max_temp).step_by(10) {
            let x = Self::temp_to_x(temp, &bounds);
            let line = Path::line(
                Point::new(x, Self::PADDING_TOP),
                Point::new(x, bounds.height - Self::PADDING_BOTTOM),
            );
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(1.0)
                    .with_color(colors::BG_OVERLAY),
            );

            // Temperature label
            if temp % 20 == 0 {
                frame.fill_text(Text {
                    content: format!("{}°", temp),
                    position: Point::new(x, bounds.height - Self::PADDING_BOTTOM + 12.0),
                    color: colors::TEXT_MUTED,
                    size: 12.0.into(),
                    horizontal_alignment: Horizontal::Center,
                    vertical_alignment: Vertical::Top,
                    ..Text::default()
                });
            }
        }

        // Draw grid lines (speed)
        let (min_speed, max_speed) = Self::speed_range();
        for speed in (min_speed..=max_speed).step_by(25) {
            let y = Self::speed_to_y(speed, &bounds);
            let line = Path::line(
                Point::new(Self::PADDING_LEFT, y),
                Point::new(bounds.width - Self::PADDING_RIGHT, y),
            );
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(1.0)
                    .with_color(colors::BG_OVERLAY),
            );

            // Speed label - only draw 0%, 50%, 100%
            if speed == 0 || speed == 50 || speed == 100 {
                frame.fill_text(Text {
                    content: format!("{}%", speed),
                    position: Point::new(Self::PADDING_LEFT - 5.0, y),
                    color: colors::TEXT_MUTED,
                    size: 12.0.into(),
                    horizontal_alignment: Horizontal::Right,
                    vertical_alignment: Vertical::Center,
                    ..Text::default()
                });
            }
        }

        // Draw current temperature indicator
        if let Some(temp) = self.current_temp {
            let x = Self::temp_to_x(temp, &bounds);
            let current_speed = self.curve.speed_for_temperature(temp);
            let y = Self::speed_to_y(current_speed.as_percentage(), &bounds);

            // Vertical line at current temp
            let temp_line = Path::line(
                Point::new(x, Self::PADDING_TOP),
                Point::new(x, bounds.height - Self::PADDING_BOTTOM),
            );
            frame.stroke(
                &temp_line,
                Stroke::default()
                    .with_width(2.0)
                    .with_color(colors::ACCENT_CYAN),
            );

            // Circle at current operating point
            let point = Path::circle(Point::new(x, y), 8.0);
            frame.fill(&point, colors::ACCENT_CYAN);
        }

        // Draw the curve line
        let points = self.curve.points();
        if !points.is_empty() {
            // Draw from left edge to first point
            let first_point = &points[0];
            let default_y = Self::speed_to_y(self.curve.default_speed().as_percentage(), &bounds);
            let first_x = Self::temp_to_x(first_point.temperature, &bounds);
            let first_y = Self::speed_to_y(first_point.speed.as_percentage(), &bounds);

            // Horizontal line from left to first point's temp
            let start_line = Path::line(
                Point::new(Self::PADDING_LEFT, default_y),
                Point::new(first_x, default_y),
            );
            frame.stroke(
                &start_line,
                Stroke::default()
                    .with_width(3.0)
                    .with_color(colors::ACCENT_GREEN),
            );

            // Vertical step up to first point
            if (default_y - first_y).abs() > 0.1 {
                let step = Path::line(Point::new(first_x, default_y), Point::new(first_x, first_y));
                frame.stroke(
                    &step,
                    Stroke::default()
                        .with_width(3.0)
                        .with_color(colors::ACCENT_GREEN),
                );
            }

            // Draw segments between points (step function)
            for i in 0..points.len() {
                let point = &points[i];
                let x = Self::temp_to_x(point.temperature, &bounds);
                let y = Self::speed_to_y(point.speed.as_percentage(), &bounds);

                // Horizontal line to next point (or edge)
                let end_x = if i + 1 < points.len() {
                    Self::temp_to_x(points[i + 1].temperature, &bounds)
                } else {
                    bounds.width - Self::PADDING_RIGHT
                };

                let h_line = Path::line(Point::new(x, y), Point::new(end_x, y));
                frame.stroke(
                    &h_line,
                    Stroke::default()
                        .with_width(3.0)
                        .with_color(colors::ACCENT_GREEN),
                );

                // Vertical step to next point
                if i + 1 < points.len() {
                    let next_y = Self::speed_to_y(points[i + 1].speed.as_percentage(), &bounds);
                    if (y - next_y).abs() > 0.1 {
                        let v_line = Path::line(Point::new(end_x, y), Point::new(end_x, next_y));
                        frame.stroke(
                            &v_line,
                            Stroke::default()
                                .with_width(3.0)
                                .with_color(colors::ACCENT_GREEN),
                        );
                    }
                }
            }

            // Draw curve points with hover/drag effects
            for (i, point) in points.iter().enumerate() {
                let x = Self::temp_to_x(point.temperature, &bounds);
                let y = Self::speed_to_y(point.speed.as_percentage(), &bounds);

                // Determine point state and color
                let is_dragging = state.dragging == Some(i);
                let is_hovered = state.hovered == Some(i);

                let (color, radius) = if is_dragging {
                    (colors::ACCENT_ORANGE, POINT_RADIUS + 3.0)
                } else if is_hovered {
                    (colors::ACCENT_CYAN, POINT_RADIUS + 2.0)
                } else {
                    (colors::ACCENT_GREEN, POINT_RADIUS)
                };

                // Outer circle
                let outer = Path::circle(Point::new(x, y), radius);
                frame.fill(&outer, color);

                // Inner circle
                let inner = Path::circle(Point::new(x, y), POINT_INNER_RADIUS);
                frame.fill(&inner, colors::BG_SURFACE);

                // Show tooltip with values when hovering or dragging
                if is_hovered || is_dragging {
                    let tooltip =
                        format!("{}°C → {}%", point.temperature, point.speed.as_percentage());
                    frame.fill_text(Text {
                        content: tooltip,
                        position: Point::new(x, y - radius - 8.0),
                        color: colors::TEXT_PRIMARY,
                        size: 11.0.into(),
                        horizontal_alignment: Horizontal::Center,
                        vertical_alignment: Vertical::Bottom,
                        ..Text::default()
                    });
                }
            }
        }

        // Draw axis labels
        frame.fill_text(Text {
            content: "Temperature".to_string(),
            position: Point::new(bounds.width / 2.0, bounds.height - 5.0),
            color: colors::TEXT_SECONDARY,
            size: 12.0.into(),
            horizontal_alignment: Horizontal::Center,
            vertical_alignment: Vertical::Bottom,
            ..Text::default()
        });

        // Show drag cursor hint
        if state.dragging.is_some() {
            if let Some(pos) = cursor.position_in(bounds) {
                let temp = Self::x_to_temp(pos.x, &bounds);
                let speed = Self::y_to_speed(pos.y, &bounds);
                let hint = format!("{}°C, {}%", temp, speed);
                frame.fill_text(Text {
                    content: hint,
                    position: Point::new(pos.x, pos.y + 20.0),
                    color: colors::TEXT_PRIMARY,
                    size: 10.0.into(),
                    horizontal_alignment: Horizontal::Center,
                    vertical_alignment: Vertical::Top,
                    ..Text::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging.is_some() {
            return mouse::Interaction::Grabbing;
        }

        if let Some(pos) = cursor.position_in(bounds) {
            if self.point_at_position(pos, &bounds).is_some() {
                return mouse::Interaction::Grab;
            }
        }

        mouse::Interaction::default()
    }
}

/// Simple fan speed slider widget
#[allow(dead_code)]
pub struct FanSpeedSlider {
    speed: u8,
    min: u8,
    max: u8,
}

#[allow(dead_code)]
impl FanSpeedSlider {
    /// Create a new fan speed slider
    pub fn new(speed: u8) -> Self {
        Self {
            speed: speed.min(100),
            min: 0,
            max: 100,
        }
    }

    /// Set the minimum speed
    #[allow(dead_code)]
    pub fn with_min(mut self, min: u8) -> Self {
        self.min = min.min(100);
        self
    }

    /// Set the maximum speed
    #[allow(dead_code)]
    pub fn with_max(mut self, max: u8) -> Self {
        self.max = max.min(100);
        self
    }
}

impl canvas::Program<Message> for FanSpeedSlider {
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

        let padding = 20.0;
        let bar_height = 20.0;
        let bar_y = bounds.height / 2.0 - bar_height / 2.0;
        let bar_width = bounds.width - padding * 2.0;

        // Draw background bar
        let bg = Path::new(|builder| {
            builder.move_to(Point::new(padding, bar_y));
            builder.line_to(Point::new(padding + bar_width, bar_y));
            builder.line_to(Point::new(padding + bar_width, bar_y + bar_height));
            builder.line_to(Point::new(padding, bar_y + bar_height));
            builder.close();
        });
        frame.fill(&bg, colors::BG_ELEVATED);

        // Draw filled portion
        let ratio = (self.speed - self.min) as f32 / (self.max - self.min) as f32;
        let fill_width = bar_width * ratio;
        let color = colors::fan_color(self.speed);

        if fill_width > 0.0 {
            let fill = Path::new(|builder| {
                builder.move_to(Point::new(padding, bar_y));
                builder.line_to(Point::new(padding + fill_width, bar_y));
                builder.line_to(Point::new(padding + fill_width, bar_y + bar_height));
                builder.line_to(Point::new(padding, bar_y + bar_height));
                builder.close();
            });
            frame.fill(&fill, color);
        }

        // Draw border
        let border = Path::new(|builder| {
            builder.move_to(Point::new(padding, bar_y));
            builder.line_to(Point::new(padding + bar_width, bar_y));
            builder.line_to(Point::new(padding + bar_width, bar_y + bar_height));
            builder.line_to(Point::new(padding, bar_y + bar_height));
            builder.close();
        });
        frame.stroke(
            &border,
            Stroke::default()
                .with_width(1.0)
                .with_color(colors::BG_OVERLAY),
        );

        // Draw handle
        let handle_x = padding + fill_width;
        let handle = Path::circle(
            Point::new(handle_x, bounds.height / 2.0),
            bar_height / 2.0 + 4.0,
        );
        frame.fill(&handle, color);

        let inner_handle = Path::circle(
            Point::new(handle_x, bounds.height / 2.0),
            bar_height / 2.0 - 2.0,
        );
        frame.fill(&inner_handle, colors::BG_SURFACE);

        // Draw speed text
        frame.fill_text(Text {
            content: format!("{}%", self.speed),
            position: Point::new(handle_x - 12.0, bounds.height / 2.0 - 6.0),
            color: colors::TEXT_PRIMARY,
            size: 14.0.into(),
            ..Text::default()
        });

        // Draw min/max labels
        frame.fill_text(Text {
            content: format!("{}%", self.min),
            position: Point::new(padding - 5.0, bar_y + bar_height + 8.0),
            color: colors::TEXT_MUTED,
            size: 11.0.into(),
            ..Text::default()
        });

        frame.fill_text(Text {
            content: format!("{}%", self.max),
            position: Point::new(padding + bar_width - 20.0, bar_y + bar_height + 8.0),
            color: colors::TEXT_MUTED,
            size: 11.0.into(),
            ..Text::default()
        });

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fan_curve_editor_creation() {
        let curve = FanCurve::default();
        let editor = FanCurveEditor::new(curve);
        assert!(editor.current_temp.is_none());
    }

    #[test]
    fn test_editor_state_default() {
        let state = EditorState::default();
        assert!(state.dragging.is_none());
        assert!(state.hovered.is_none());
    }

    #[test]
    fn test_fan_speed_slider_creation() {
        let slider = FanSpeedSlider::new(50);
        assert_eq!(slider.speed, 50);
    }

    #[test]
    fn test_fan_speed_slider_clamping() {
        let slider = FanSpeedSlider::new(150);
        assert_eq!(slider.speed, 100);
    }

    #[test]
    fn test_coordinate_conversion() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 500.0,
            height: 300.0,
        };

        // Test temp range edges
        let x_min = FanCurveEditor::temp_to_x(20, &bounds);
        let x_max = FanCurveEditor::temp_to_x(100, &bounds);
        assert!(x_min < x_max);
        assert!(x_min >= FanCurveEditor::PADDING_LEFT);
        assert!(x_max <= 500.0 - FanCurveEditor::PADDING_RIGHT);

        // Test speed range edges
        let y_min = FanCurveEditor::speed_to_y(100, &bounds);
        let y_max = FanCurveEditor::speed_to_y(0, &bounds);
        assert!(y_min < y_max); // Y is inverted
        assert!(y_min >= FanCurveEditor::PADDING_TOP);
        assert!(y_max <= 300.0 - FanCurveEditor::PADDING_BOTTOM);
    }

    #[test]
    fn test_coordinate_roundtrip() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 500.0,
            height: 300.0,
        };

        // Test temperature roundtrip
        let temp = 50;
        let x = FanCurveEditor::temp_to_x(temp, &bounds);
        let temp_back = FanCurveEditor::x_to_temp(x, &bounds);
        assert_eq!(temp, temp_back);

        // Test speed roundtrip
        let speed = 75u8;
        let y = FanCurveEditor::speed_to_y(speed, &bounds);
        let speed_back = FanCurveEditor::y_to_speed(y, &bounds);
        assert_eq!(speed, speed_back);
    }
}
