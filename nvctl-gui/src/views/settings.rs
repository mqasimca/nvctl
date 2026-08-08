//! Settings view
//!
//! Application settings and configuration.

use crate::message::{Message, SettingsMessage};
use crate::services::GuiConfig;
use crate::state::{AppState, NotificationLevel};
use crate::theme::{colors, font_size, spacing};

use iced::widget::{column, container, horizontal_space, row, scrollable, slider, text, toggler};
use iced::{Alignment, Element, Length};

/// Render the settings view
pub fn view_settings<'a>(state: &'a AppState, config: &'a GuiConfig) -> Element<'a, Message> {
    let header = view_header();
    let about = view_about();
    let keyboard_shortcuts = view_keyboard_shortcuts();
    let preferences = view_preferences(config);
    let activity = view_recent_activity(state);
    let gpu_list = view_gpu_list(state);
    let app_info = view_app_info(config.preferences.poll_interval_ms);

    let content = column![
        header,
        about,
        preferences,
        activity,
        keyboard_shortcuts,
        gpu_list,
        app_info
    ]
    .spacing(spacing::LG)
    .padding(spacing::LG)
    .width(Length::Fill);

    scrollable(content).height(Length::Fill).into()
}

/// Recent control feedback remains available after a transient toast closes.
fn view_recent_activity(state: &AppState) -> Element<'_, Message> {
    let title = text("Recent Activity")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let entries: Vec<Element<'_, Message>> = if state.notification_history.is_empty() {
        vec![text("No control actions or alerts in this session.")
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
            .into()]
    } else {
        state
            .notification_history
            .iter()
            .rev()
            .take(8)
            .map(|notification| {
                let (label, color) = match notification.level {
                    NotificationLevel::Success => ("Success", colors::ACCENT_GREEN),
                    NotificationLevel::Warning => ("Warning", colors::ACCENT_ORANGE),
                    NotificationLevel::Error => ("Error", colors::ACCENT_RED),
                };
                row![
                    text(label).size(font_size::XS).color(color),
                    text(&notification.message)
                        .size(font_size::SM)
                        .color(colors::TEXT_SECONDARY),
                ]
                .spacing(spacing::SM)
                .into()
            })
            .collect()
    };

    container(
        column![
            title,
            iced::widget::Column::with_children(entries).spacing(spacing::XS)
        ]
        .spacing(spacing::MD),
    )
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

/// Preferences that affect the local application. Values save as they change.
fn view_preferences(config: &GuiConfig) -> Element<'_, Message> {
    let title = text("Application Preferences")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let poll_interval = config.preferences.poll_interval_ms.clamp(250, 5_000);
    let poll_control = column![
        row![
            text("Telemetry update rate")
                .size(font_size::BASE)
                .color(colors::TEXT_SECONDARY),
            horizontal_space(),
            text(format!("{} ms", poll_interval))
                .size(font_size::BASE)
                .color(colors::ACCENT_CYAN),
        ]
        .align_y(Alignment::Center),
        slider(250..=5_000, poll_interval as i32, |value| {
            Message::Settings(SettingsMessage::PollInterval(value as u64))
        })
        .step(250)
        .width(Length::Fill),
        text("Lower intervals refresh more often and use more system resources.")
            .size(font_size::SM)
            .color(colors::TEXT_MUTED),
    ]
    .spacing(spacing::SM);

    let tray = toggler(config.preferences.enable_tray)
        .label("Enable system tray")
        .on_toggle(|enabled| Message::Settings(SettingsMessage::TrayEnabled(enabled)))
        .size(20.0);
    let start_minimized = toggler(config.preferences.start_minimized)
        .label("Start minimized to tray")
        .on_toggle(|enabled| Message::Settings(SettingsMessage::StartMinimized(enabled)))
        .size(20.0);

    container(
        column![
            title,
            poll_control,
            tray,
            start_minimized,
            text("Tray changes take effect after restarting nvctl.")
                .size(font_size::SM)
                .color(colors::TEXT_MUTED),
        ]
        .spacing(spacing::MD),
    )
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
fn view_app_info(poll_interval_ms: u64) -> Element<'static, Message> {
    let title = text("System Information")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let features = column![
        view_info_row("Fan Control", "Manual speed and curve-based control"),
        view_info_row("Power Control", "Adjustable power limits"),
        view_info_row("Thermal Monitoring", "Real-time temperature tracking"),
        view_info_row(
            "Update Rate",
            format!("{} ms polling interval", poll_interval_ms)
        ),
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

fn view_info_row(label: &'static str, value: impl Into<String>) -> Element<'static, Message> {
    row![
        text(label)
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(value.into())
            .size(font_size::SM)
            .color(colors::TEXT_PRIMARY),
    ]
    .align_y(Alignment::Center)
    .into()
}
