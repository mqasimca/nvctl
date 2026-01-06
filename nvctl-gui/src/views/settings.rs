//! Settings view
//!
//! Application settings and configuration.

use crate::message::Message;
use crate::state::AppState;
use crate::theme::{colors, font_size, spacing};

use iced::widget::{column, container, horizontal_space, row, scrollable, text};
use iced::{Alignment, Element, Length};

/// Render the settings view
pub fn view_settings(state: &AppState) -> Element<'_, Message> {
    let header = view_header();
    let about = view_about();
    let keyboard_shortcuts = view_keyboard_shortcuts();
    let gpu_list = view_gpu_list(state);
    let app_info = view_app_info();

    let content = column![header, about, keyboard_shortcuts, gpu_list, app_info]
        .spacing(spacing::LG)
        .padding(spacing::LG)
        .width(Length::Fill);

    scrollable(content).height(Length::Fill).into()
}

/// Settings header
fn view_header() -> Element<'static, Message> {
    let title = text("Settings")
        .size(font_size::XXL)
        .color(colors::TEXT_PRIMARY);

    row![title, horizontal_space()]
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// About section
fn view_about() -> Element<'static, Message> {
    let title = text("About nvctl")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let description = text("NVML-based GPU control tool for NVIDIA GPUs")
        .size(font_size::BASE)
        .color(colors::TEXT_SECONDARY);

    let version = text(format!("Version: {}", env!("CARGO_PKG_VERSION")))
        .size(font_size::SM)
        .color(colors::TEXT_MUTED);

    container(column![title, description, version].spacing(spacing::SM))
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

/// Keyboard shortcuts section
fn view_keyboard_shortcuts() -> Element<'static, Message> {
    let title = text("Keyboard Shortcuts")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let shortcuts = column![
        view_shortcut_row("Ctrl+1", "Go to Dashboard"),
        view_shortcut_row("Ctrl+2", "Go to Fan Control"),
        view_shortcut_row("Ctrl+3", "Go to Power"),
        view_shortcut_row("Ctrl+4", "Go to Thermal"),
        view_shortcut_row("Ctrl+5", "Go to Profiles"),
        view_shortcut_row("Ctrl+,", "Go to Settings"),
        view_shortcut_row("Ctrl+B", "Toggle Sidebar"),
        view_shortcut_row("F5", "Refresh GPU Data"),
    ]
    .spacing(spacing::XS);

    container(column![title, shortcuts].spacing(spacing::MD))
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

/// Single shortcut row
fn view_shortcut_row<'a>(shortcut: &'a str, description: &'a str) -> Element<'a, Message> {
    row![
        container(
            text(shortcut)
                .size(font_size::SM)
                .color(colors::ACCENT_CYAN)
        )
        .padding([spacing::XS, spacing::SM])
        .style(|_theme| container::Style {
            background: Some(colors::BG_ELEVATED.into()),
            border: iced::Border {
                color: colors::BG_SURFACE,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        }),
        horizontal_space().width(Length::Fixed(spacing::MD as f32)),
        text(description)
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// GPU list section
fn view_gpu_list(state: &AppState) -> Element<'_, Message> {
    let title = text("Detected GPUs")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let gpus: Vec<Element<'_, Message>> = if state.gpus.is_empty() {
        vec![text("No NVIDIA GPUs detected")
            .size(font_size::BASE)
            .color(colors::TEXT_MUTED)
            .into()]
    } else {
        state
            .gpus
            .iter()
            .enumerate()
            .map(|(i, gpu)| {
                let is_selected = i == state.selected_gpu;
                let indicator = if is_selected { ">" } else { " " };
                let color = if is_selected {
                    colors::ACCENT_CYAN
                } else {
                    colors::TEXT_PRIMARY
                };

                row![
                    text(indicator)
                        .size(font_size::BASE)
                        .color(colors::ACCENT_CYAN),
                    text(format!("GPU {}: {}", gpu.index, gpu.info.name))
                        .size(font_size::BASE)
                        .color(color),
                    horizontal_space(),
                    text(format!("{}°C", gpu.temperature.as_celsius()))
                        .size(font_size::SM)
                        .color(colors::temp_color(gpu.temperature.as_celsius())),
                ]
                .spacing(spacing::SM)
                .align_y(Alignment::Center)
                .into()
            })
            .collect()
    };

    let gpu_column = iced::widget::Column::with_children(gpus).spacing(spacing::SM);

    container(column![title, gpu_column].spacing(spacing::MD))
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

/// Application info
fn view_app_info() -> Element<'static, Message> {
    let title = text("System Information")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let features = column![
        view_info_row("Fan Control", "Manual speed and curve-based control"),
        view_info_row("Power Control", "Adjustable power limits"),
        view_info_row("Thermal Monitoring", "Real-time temperature tracking"),
        view_info_row("Update Rate", "1 second polling interval"),
    ]
    .spacing(spacing::SM);

    container(column![title, features].spacing(spacing::MD))
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

fn view_info_row<'a>(label: &'a str, value: &'a str) -> Element<'a, Message> {
    row![
        text(label)
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(value).size(font_size::SM).color(colors::TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center)
    .into()
}
