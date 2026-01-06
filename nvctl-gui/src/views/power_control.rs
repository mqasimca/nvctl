//! Power control view
//!
//! Provides power limit adjustment and power usage monitoring.

use crate::message::{Message, PowerControlMessage};
use crate::state::{AppState, GpuState};
use crate::theme::{colors, font_size, spacing};
use crate::widgets::PowerBar;

use iced::widget::{button, column, container, horizontal_space, row, slider, text, Canvas};
use iced::{Alignment, Element, Length, Theme};
use nvctl::domain::PowerLimit;

/// Render the power control view
pub fn view_power_control(state: &AppState) -> Element<'_, Message> {
    let header = view_header(state);

    let content: Element<'_, Message> = if state.gpus.is_empty() {
        view_no_gpu()
    } else if let Some(gpu) = state.current_gpu() {
        view_power_control_content(state, gpu)
    } else {
        view_no_gpu()
    };

    column![header, content]
        .spacing(spacing::LG)
        .padding(spacing::LG)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Power control header
fn view_header(state: &AppState) -> Element<'_, Message> {
    let title = text("Power Control")
        .size(font_size::XXL)
        .color(colors::TEXT_PRIMARY);

    // Linked mode indicator
    let linked_indicator: Element<'_, Message> = if state.has_multiple_gpus() && state.linked_gpus {
        row![
            text("🔗").size(font_size::BASE),
            text(format!("Applies to all {} GPUs", state.gpus.len()))
                .size(font_size::SM)
                .color(colors::ACCENT_CYAN),
        ]
        .spacing(spacing::XS)
        .align_y(Alignment::Center)
        .into()
    } else {
        text("").into()
    };

    let gpu_info: Element<'_, Message> = if let Some(gpu) = state.current_gpu() {
        text(gpu.info.short_name())
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY)
            .into()
    } else {
        text("No GPU")
            .size(font_size::BASE)
            .color(colors::TEXT_MUTED)
            .into()
    };

    row![title, horizontal_space(), linked_indicator, gpu_info]
        .align_y(Alignment::Center)
        .spacing(spacing::MD)
        .width(Length::Fill)
        .into()
}

/// View when no GPU available
fn view_no_gpu() -> Element<'static, Message> {
    container(
        column![
            text("No GPU Available")
                .size(font_size::XL)
                .color(colors::TEXT_SECONDARY),
            text("Connect an NVIDIA GPU to control power settings.")
                .size(font_size::BASE)
                .color(colors::TEXT_MUTED),
        ]
        .spacing(spacing::MD)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

/// Main power control content
fn view_power_control_content<'a>(state: &'a AppState, gpu: &'a GpuState) -> Element<'a, Message> {
    let power_status = view_power_status(gpu);
    let power_slider = view_power_slider(state, gpu);
    let constraints_info = view_constraints(gpu);

    column![power_status, power_slider, constraints_info]
        .spacing(spacing::LG)
        .width(Length::Fill)
        .into()
}

/// Current power status display
fn view_power_status(gpu: &GpuState) -> Element<'_, Message> {
    let title = text("Current Power Usage")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let power_widget = PowerBar::new(gpu.power_usage, gpu.power_limit, gpu.power_constraints);
    let power_canvas: Element<'_, Message> = Canvas::new(power_widget)
        .width(Length::Fixed(180.0))
        .height(Length::Fixed(180.0))
        .into();

    // Center the gauge
    let gauge_centered = container(power_canvas)
        .width(Length::Fill)
        .center_x(Length::Fill);

    let usage_row = row![
        text("Usage:")
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(format!("{}W", gpu.power_usage.as_watts()))
            .size(font_size::XL)
            .color(colors::power_color(gpu.power_ratio())),
        text(format!(" / {}W", gpu.power_limit.as_watts()))
            .size(font_size::BASE)
            .color(colors::TEXT_MUTED),
    ]
    .align_y(Alignment::Center);

    let percentage = if gpu.power_limit.as_watts() > 0 {
        (gpu.power_usage.as_watts() as f32 / gpu.power_limit.as_watts() as f32 * 100.0) as u32
    } else {
        0
    };

    let percentage_row = row![
        text("Utilization:")
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(format!("{}%", percentage))
            .size(font_size::LG)
            .color(colors::power_color(gpu.power_ratio())),
    ]
    .align_y(Alignment::Center);

    container(column![title, gauge_centered, usage_row, percentage_row].spacing(spacing::MD))
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::BG_ELEVATED,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Power limit slider
fn view_power_slider<'a>(state: &'a AppState, gpu: &'a GpuState) -> Element<'a, Message> {
    let title = text("Power Limit")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let (min_watts, max_watts) = if let Some(ref constraints) = gpu.power_constraints {
        (constraints.min.as_watts(), constraints.max.as_watts())
    } else {
        // Default range if no constraints
        (100, 350)
    };

    let current_watts = gpu.power_limit.as_watts();

    let slider_row = row![
        text(format!("{}W", min_watts))
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
            .width(Length::Fixed(50.0)),
        slider(
            min_watts as i32..=max_watts as i32,
            current_watts as i32,
            |value| {
                Message::PowerControl(PowerControlMessage::LimitChanged(PowerLimit::from_watts(
                    value as u32,
                )))
            }
        )
        .width(Length::Fill),
        text(format!("{}W", max_watts))
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
            .width(Length::Fixed(50.0)),
    ]
    .align_y(Alignment::Center)
    .spacing(spacing::MD);

    let current_display = row![
        text("Current Limit:")
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(format!("{}W", current_watts))
            .size(font_size::XL)
            .color(colors::ACCENT_CYAN),
    ]
    .align_y(Alignment::Center);

    // Apply button - shows "Apply to All" when in linked mode
    let apply_label = if state.linked_gpus && state.has_multiple_gpus() {
        format!("Apply to All {} GPUs", state.gpus.len())
    } else {
        "Apply".to_string()
    };

    let buttons_row = row![
        button(text("Reset to Default").size(font_size::SM))
            .on_press(Message::PowerControl(PowerControlMessage::ResetToDefault))
            .padding([spacing::SM, spacing::MD])
            .style(secondary_button_style),
        horizontal_space(),
        button(text(apply_label).size(font_size::BASE))
            .on_press(Message::PowerControl(PowerControlMessage::ApplyLimit))
            .padding([spacing::SM, spacing::MD])
            .style(if state.linked_gpus && state.has_multiple_gpus() {
                linked_apply_button_style
            } else {
                primary_button_style
            }),
    ]
    .width(Length::Fill);

    container(column![title, slider_row, current_display, buttons_row].spacing(spacing::MD))
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::BG_ELEVATED,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Power constraints information
fn view_constraints(gpu: &GpuState) -> Element<'_, Message> {
    let title = text("Power Constraints")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let content: Element<'_, Message> = if let Some(ref constraints) = gpu.power_constraints {
        let min_row = row![
            text("Minimum:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text(format!("{}W", constraints.min.as_watts()))
                .size(font_size::BASE)
                .color(colors::TEXT_PRIMARY),
        ]
        .align_y(Alignment::Center);

        let default_row = row![
            text("Default:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text(format!("{}W", constraints.default.as_watts()))
                .size(font_size::BASE)
                .color(colors::ACCENT_GREEN),
        ]
        .align_y(Alignment::Center);

        let max_row = row![
            text("Maximum:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text(format!("{}W", constraints.max.as_watts()))
                .size(font_size::BASE)
                .color(colors::ACCENT_ORANGE),
        ]
        .align_y(Alignment::Center);

        column![min_row, default_row, max_row]
            .spacing(spacing::SM)
            .into()
    } else {
        text("Power constraints not available for this GPU.")
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
            .into()
    };

    container(column![title, content].spacing(spacing::MD))
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::BG_ELEVATED,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

// Button style helpers

fn primary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => colors::ACCENT_GREEN,
        _ => colors::ACCENT_CYAN,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: colors::BG_BASE,
        border: iced::Border {
            color: bg,
            width: 0.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

fn secondary_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => colors::BG_OVERLAY,
        _ => colors::BG_ELEVATED,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: colors::TEXT_SECONDARY,
        border: iced::Border {
            color: colors::BG_OVERLAY,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

fn linked_apply_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => colors::ACCENT_GREEN,
        _ => colors::ACCENT_ORANGE,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: colors::BG_BASE,
        border: iced::Border {
            color: bg,
            width: 0.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}
