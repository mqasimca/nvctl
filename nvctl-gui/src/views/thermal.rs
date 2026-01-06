//! Thermal control view
//!
//! Displays temperature information and thermal thresholds.

use crate::message::Message;
use crate::state::{AppState, GpuState};
use crate::theme::{colors, font_size, spacing};
use crate::widgets::TempGauge;

use iced::widget::{column, container, horizontal_space, row, text, Canvas};
use iced::{Alignment, Element, Length};

/// Render the thermal control view
pub fn view_thermal_control(state: &AppState) -> Element<'_, Message> {
    let header = view_header(state);

    let content: Element<'_, Message> = if state.gpus.is_empty() {
        view_no_gpu()
    } else if let Some(gpu) = state.current_gpu() {
        view_thermal_content(gpu)
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

/// Thermal control header
fn view_header(state: &AppState) -> Element<'_, Message> {
    let title = text("Thermal Monitor")
        .size(font_size::XXL)
        .color(colors::TEXT_PRIMARY);

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

    row![title, horizontal_space(), gpu_info]
        .align_y(Alignment::Center)
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
            text("Connect an NVIDIA GPU to view thermal information.")
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

/// Main thermal content
fn view_thermal_content(gpu: &GpuState) -> Element<'_, Message> {
    let temp_display = view_temperature(gpu);
    let thresholds = view_thresholds(gpu);
    let thermal_status = view_thermal_status(gpu);

    column![temp_display, thresholds, thermal_status]
        .spacing(spacing::LG)
        .width(Length::Fill)
        .into()
}

/// Temperature display with gauge
fn view_temperature(gpu: &GpuState) -> Element<'_, Message> {
    let title = text("Current Temperature")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let temp_widget = TempGauge::new(gpu.temperature, gpu.thresholds);
    let temp_canvas: Element<'_, Message> = Canvas::new(temp_widget)
        .width(Length::Fixed(160.0))
        .height(Length::Fixed(160.0))
        .into();

    let temp_text = text(format!("{}°C", gpu.temperature.as_celsius()))
        .size(font_size::XXXL)
        .color(colors::temp_color(gpu.temperature.as_celsius()));

    let status_text = get_thermal_status_text(gpu.temperature.as_celsius());
    let status = text(status_text)
        .size(font_size::BASE)
        .color(colors::temp_color(gpu.temperature.as_celsius()));

    let content = row![
        temp_canvas,
        column![temp_text, status]
            .spacing(spacing::SM)
            .align_x(Alignment::Center),
    ]
    .spacing(spacing::LG)
    .align_y(Alignment::Center);

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

/// Thermal thresholds information
fn view_thresholds(gpu: &GpuState) -> Element<'_, Message> {
    let title = text("Thermal Thresholds")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let slowdown: Element<'_, Message> = if let Some(temp) = gpu.thresholds.slowdown {
        row![
            text("Slowdown Threshold:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text(format!("{}°C", temp.as_celsius()))
                .size(font_size::BASE)
                .color(colors::ACCENT_ORANGE),
        ]
        .align_y(Alignment::Center)
        .into()
    } else {
        row![
            text("Slowdown Threshold:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text("Not available")
                .size(font_size::BASE)
                .color(colors::TEXT_MUTED),
        ]
        .align_y(Alignment::Center)
        .into()
    };

    let shutdown: Element<'_, Message> = if let Some(temp) = gpu.thresholds.shutdown {
        row![
            text("Shutdown Threshold:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text(format!("{}°C", temp.as_celsius()))
                .size(font_size::BASE)
                .color(colors::ACCENT_RED),
        ]
        .align_y(Alignment::Center)
        .into()
    } else {
        row![
            text("Shutdown Threshold:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text("Not available")
                .size(font_size::BASE)
                .color(colors::TEXT_MUTED),
        ]
        .align_y(Alignment::Center)
        .into()
    };

    let max_op: Element<'_, Message> = if let Some(temp) = gpu.thresholds.gpu_max {
        row![
            text("GPU Max Temp:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text(format!("{}°C", temp.as_celsius()))
                .size(font_size::BASE)
                .color(colors::ACCENT_ORANGE),
        ]
        .align_y(Alignment::Center)
        .into()
    } else {
        row![
            text("GPU Max Temp:")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text("Not available")
                .size(font_size::BASE)
                .color(colors::TEXT_MUTED),
        ]
        .align_y(Alignment::Center)
        .into()
    };

    container(column![title, slowdown, shutdown, max_op].spacing(spacing::MD))
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

/// Thermal status and recommendations
fn view_thermal_status(gpu: &GpuState) -> Element<'_, Message> {
    let title = text("Thermal Status")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let temp = gpu.temperature.as_celsius();
    let (status, recommendation) = get_thermal_recommendation(temp);

    let status_row = row![
        text("Status:")
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(status)
            .size(font_size::BASE)
            .color(colors::temp_color(temp)),
    ]
    .align_y(Alignment::Center);

    let rec_row = row![
        text("Recommendation:")
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(recommendation)
            .size(font_size::SM)
            .color(colors::TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center);

    // Temperature history stats
    let history = &gpu.temp_history;
    let history_section: Element<'_, Message> = if !history.is_empty() {
        let min_temp = history.min as i32;
        let max_temp = history.max as i32;
        let current = history.latest().unwrap_or(0.0) as i32;

        column![
            row![
                text("Session Min:")
                    .size(font_size::SM)
                    .color(colors::TEXT_SECONDARY),
                horizontal_space(),
                text(format!("{}°C", min_temp))
                    .size(font_size::SM)
                    .color(colors::ACCENT_CYAN),
            ]
            .align_y(Alignment::Center),
            row![
                text("Session Max:")
                    .size(font_size::SM)
                    .color(colors::TEXT_SECONDARY),
                horizontal_space(),
                text(format!("{}°C", max_temp))
                    .size(font_size::SM)
                    .color(colors::temp_color(max_temp)),
            ]
            .align_y(Alignment::Center),
            row![
                text("Current:")
                    .size(font_size::SM)
                    .color(colors::TEXT_SECONDARY),
                horizontal_space(),
                text(format!("{}°C", current))
                    .size(font_size::SM)
                    .color(colors::temp_color(current)),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(spacing::XS)
        .into()
    } else {
        text("Collecting temperature data...")
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
            .into()
    };

    container(column![title, status_row, rec_row, history_section].spacing(spacing::MD))
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

/// Get thermal status text based on temperature
fn get_thermal_status_text(temp: i32) -> &'static str {
    match temp {
        t if t < 40 => "Cool",
        t if t < 55 => "Normal",
        t if t < 65 => "Warm",
        t if t < 80 => "Hot",
        _ => "Critical",
    }
}

/// Get thermal recommendation based on temperature
fn get_thermal_recommendation(temp: i32) -> (&'static str, &'static str) {
    match temp {
        t if t < 40 => (
            "Excellent",
            "GPU is running cool. Consider increasing performance.",
        ),
        t if t < 55 => ("Good", "Temperature is within normal operating range."),
        t if t < 65 => ("Acceptable", "Temperature is elevated but safe."),
        t if t < 80 => ("Warning", "Consider increasing fan speed or reducing load."),
        _ => (
            "Critical",
            "GPU is overheating! Reduce load or improve cooling.",
        ),
    }
}
