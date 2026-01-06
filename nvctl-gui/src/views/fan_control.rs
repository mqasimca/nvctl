//! Fan control view
//!
//! Provides fan speed control, fan curve editing, and policy management.

use crate::message::{CurvePreset, FanControlMessage, Message};
use crate::services::GuiConfig;
use crate::state::{AppState, GpuState};
use crate::theme::{colors, font_size, spacing};
use crate::widgets::FanCurveEditor;

use iced::widget::{button, column, container, horizontal_space, row, slider, text, Canvas};
use iced::{Alignment, Element, Length, Theme};
use nvctl::domain::{FanPolicy, FanSpeed};

/// Render the fan control view
pub fn view_fan_control<'a>(state: &'a AppState, config: &'a GuiConfig) -> Element<'a, Message> {
    let header = view_header(state);

    let content: Element<'a, Message> = if state.gpus.is_empty() {
        view_no_gpu()
    } else if let Some(gpu) = state.current_gpu() {
        view_fan_control_content(state, gpu, config)
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

/// Fan control header
fn view_header(state: &AppState) -> Element<'_, Message> {
    let title = text("Fan Control")
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
            text("Connect an NVIDIA GPU to control fan settings.")
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

/// Main fan control content
fn view_fan_control_content<'a>(
    state: &'a AppState,
    gpu: &'a GpuState,
    config: &'a GuiConfig,
) -> Element<'a, Message> {
    let fan_status = view_fan_status(gpu, config);
    let policy_controls = view_policy_controls(gpu, config);
    let manual_controls = view_manual_controls(gpu, config);
    let curve_editor = view_curve_section(state, gpu, config);

    column![fan_status, policy_controls, manual_controls, curve_editor]
        .spacing(spacing::LG)
        .width(Length::Fill)
        .into()
}

/// Current fan status display
fn view_fan_status<'a>(gpu: &'a GpuState, config: &'a GuiConfig) -> Element<'a, Message> {
    let title = text("Current Status")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    // Get fan config for this GPU
    let fan_config = config.gpu_fan_config(&gpu.info.uuid);

    let fans: Vec<Element<'a, Message>> = gpu
        .fan_speeds
        .iter()
        .enumerate()
        .map(|(i, speed)| {
            let policy = gpu.fan_policies.get(i).copied().unwrap_or(FanPolicy::Auto);
            let policy_text = match policy {
                FanPolicy::Auto => "Auto",
                FanPolicy::Manual => "Manual",
            };
            let policy_color = match policy {
                FanPolicy::Auto => colors::ACCENT_GREEN,
                FanPolicy::Manual => colors::ACCENT_ORANGE,
            };

            let speed_pct = speed.as_percentage();
            let speed_color = colors::fan_color(speed_pct);

            // Get fan name from config
            let fan_name = fan_config
                .map(|fc| fc.get_fan_name(i as u32, None))
                .unwrap_or_else(|| format!("Fan {}", i + 1));

            row![
                text(fan_name)
                    .size(font_size::BASE)
                    .color(colors::TEXT_SECONDARY),
                horizontal_space(),
                text(format!("{}%", speed_pct))
                    .size(font_size::LG)
                    .color(speed_color),
                text(format!(" ({})", policy_text))
                    .size(font_size::SM)
                    .color(policy_color),
            ]
            .align_y(Alignment::Center)
            .spacing(spacing::SM)
            .into()
        })
        .collect();

    let fans_column = iced::widget::Column::with_children(fans).spacing(spacing::SM);

    let temp_indicator = row![
        text("Temperature:")
            .size(font_size::BASE)
            .color(colors::TEXT_SECONDARY),
        horizontal_space(),
        text(format!("{}°C", gpu.temperature.as_celsius()))
            .size(font_size::LG)
            .color(colors::temp_color(gpu.temperature.as_celsius())),
    ]
    .align_y(Alignment::Center);

    container(column![title, fans_column, temp_indicator].spacing(spacing::MD))
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

/// Fan policy toggle controls
fn view_policy_controls<'a>(gpu: &'a GpuState, config: &'a GuiConfig) -> Element<'a, Message> {
    let title = text("Fan Policy")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    // Get fan config for this GPU
    let fan_config = config.gpu_fan_config(&gpu.info.uuid);

    let fans: Vec<Element<'a, Message>> = gpu
        .fan_policies
        .iter()
        .enumerate()
        .map(|(i, policy)| {
            let fan_idx = i as u32;

            let auto_style = if *policy == FanPolicy::Auto {
                active_button_style
            } else {
                inactive_button_style
            };

            let manual_style = if *policy == FanPolicy::Manual {
                active_button_style
            } else {
                inactive_button_style
            };

            // Get fan name from config
            let fan_name = fan_config
                .map(|fc| fc.get_fan_name(i as u32, None))
                .unwrap_or_else(|| format!("Fan {}", i + 1));

            row![
                text(format!("{}:", fan_name))
                    .size(font_size::BASE)
                    .color(colors::TEXT_SECONDARY),
                horizontal_space(),
                button(text("Auto").size(font_size::SM))
                    .on_press(Message::FanControl(FanControlMessage::PolicyChanged(
                        fan_idx,
                        FanPolicy::Auto
                    )))
                    .padding([spacing::XS, spacing::SM])
                    .style(auto_style),
                button(text("Manual").size(font_size::SM))
                    .on_press(Message::FanControl(FanControlMessage::PolicyChanged(
                        fan_idx,
                        FanPolicy::Manual
                    )))
                    .padding([spacing::XS, spacing::SM])
                    .style(manual_style),
            ]
            .align_y(Alignment::Center)
            .spacing(spacing::SM)
            .into()
        })
        .collect();

    let fans_column = iced::widget::Column::with_children(fans).spacing(spacing::SM);

    container(column![title, fans_column].spacing(spacing::MD))
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

/// Manual speed controls for fans in manual mode
fn view_manual_controls<'a>(gpu: &'a GpuState, config: &'a GuiConfig) -> Element<'a, Message> {
    let title = text("Manual Speed Control")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let has_manual = gpu.fan_policies.contains(&FanPolicy::Manual);

    if !has_manual {
        return container(
            column![
                title,
                text("Set fan policy to Manual to adjust speeds directly.")
                    .size(font_size::SM)
                    .color(colors::TEXT_MUTED),
            ]
            .spacing(spacing::SM),
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
        .into();
    }

    // Get fan config for this GPU
    let fan_config = config.gpu_fan_config(&gpu.info.uuid);

    let sliders: Vec<Element<'a, Message>> = gpu
        .fan_speeds
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            gpu.fan_policies
                .get(*i)
                .is_some_and(|p| *p == FanPolicy::Manual)
        })
        .map(|(i, speed)| {
            let fan_idx = i as u32;
            let speed_pct = speed.as_percentage();

            // Get fan name from config
            let fan_name = fan_config
                .map(|fc| fc.get_fan_name(i as u32, None))
                .unwrap_or_else(|| format!("Fan {}", i + 1));

            row![
                text(format!("{}:", fan_name))
                    .size(font_size::BASE)
                    .color(colors::TEXT_SECONDARY)
                    .width(Length::Fixed(120.0)),
                slider(0..=100, speed_pct as i32, move |value| {
                    if let Ok(speed) = FanSpeed::new(value as u8) {
                        Message::FanControl(FanControlMessage::SpeedChanged(fan_idx, speed))
                    } else {
                        Message::Error("Invalid fan speed".to_string())
                    }
                })
                .width(Length::Fill),
                text(format!("{}%", speed_pct))
                    .size(font_size::BASE)
                    .color(colors::fan_color(speed_pct))
                    .width(Length::Fixed(50.0)),
            ]
            .align_y(Alignment::Center)
            .spacing(spacing::MD)
            .into()
        })
        .collect();

    let sliders_column = iced::widget::Column::with_children(sliders).spacing(spacing::SM);

    container(column![title, sliders_column].spacing(spacing::MD))
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

/// Fan curve editor section
fn view_curve_section<'a>(
    state: &'a AppState,
    gpu: &'a GpuState,
    config: &'a GuiConfig,
) -> Element<'a, Message> {
    let title = text("Fan Curves")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let fan_count = gpu.fan_speeds.len();
    let selected_fan = state.selected_curve_fan;

    // Get fan config for this GPU
    let fan_config = config.gpu_fan_config(&gpu.info.uuid);

    // Fan selector tabs
    let fan_tabs: Vec<Element<'a, Message>> = (0..fan_count)
        .map(|i| {
            let fan_name = fan_config
                .map(|fc| fc.get_fan_name(i as u32, None))
                .unwrap_or_else(|| format!("Fan {}", i + 1));

            let is_selected = i == selected_fan;
            let style = if is_selected {
                active_button_style
            } else {
                inactive_button_style
            };

            button(text(fan_name).size(font_size::SM))
                .on_press(Message::FanControl(FanControlMessage::SelectCurveFan(i)))
                .padding([spacing::XS, spacing::SM])
                .style(style)
                .into()
        })
        .collect();

    let fan_selector = iced::widget::Row::with_children(fan_tabs)
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

    // Get curve for selected fan
    let curve = state
        .get_selected_curve()
        .or_else(|| gpu.fan_curves.get(selected_fan).cloned())
        .unwrap_or_default();

    // Curve editor canvas
    let editor = FanCurveEditor::new(curve).with_current_temp(gpu.temperature.as_celsius());
    let canvas: Element<'_, Message> = Canvas::new(editor)
        .width(Length::Fill)
        .height(Length::Fixed(250.0))
        .into();

    // Preset buttons
    let presets = row![
        text("Presets:")
            .size(font_size::SM)
            .color(colors::TEXT_SECONDARY),
        button(text("Silent").size(font_size::SM))
            .on_press(Message::FanControl(FanControlMessage::PresetSelected(
                CurvePreset::Silent
            )))
            .padding([spacing::XS, spacing::SM])
            .style(inactive_button_style),
        button(text("Balanced").size(font_size::SM))
            .on_press(Message::FanControl(FanControlMessage::PresetSelected(
                CurvePreset::Balanced
            )))
            .padding([spacing::XS, spacing::SM])
            .style(inactive_button_style),
        button(text("Performance").size(font_size::SM))
            .on_press(Message::FanControl(FanControlMessage::PresetSelected(
                CurvePreset::Performance
            )))
            .padding([spacing::XS, spacing::SM])
            .style(inactive_button_style),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    // Curve control toggle for selected fan
    let is_enabled = state.is_curve_enabled(selected_fan);
    let curve_toggle = row![
        text("Enable Curve Control:")
            .size(font_size::SM)
            .color(colors::TEXT_SECONDARY),
        button(text(if is_enabled { "Enabled" } else { "Disabled" }).size(font_size::SM))
            .on_press(Message::FanControl(FanControlMessage::CurveControlToggled(
                !is_enabled
            )))
            .padding([spacing::XS, spacing::SM])
            .style(if is_enabled {
                active_button_style
            } else {
                inactive_button_style
            }),
    ]
    .spacing(spacing::SM)
    .align_y(Alignment::Center);

    // Apply button
    let selected_fan_name = fan_config
        .map(|fc| fc.get_fan_name(selected_fan as u32, None))
        .unwrap_or_else(|| format!("Fan {}", selected_fan + 1));

    let apply_label = if state.linked_gpus && state.has_multiple_gpus() {
        format!("Apply {} to All GPUs", selected_fan_name)
    } else {
        format!("Apply to {}", selected_fan_name)
    };

    let apply_row = row![
        horizontal_space(),
        button(text(apply_label).size(font_size::BASE))
            .on_press(Message::FanControl(FanControlMessage::ApplyCurve))
            .padding([spacing::SM, spacing::MD])
            .style(if state.linked_gpus && state.has_multiple_gpus() {
                linked_apply_button_style
            } else {
                primary_button_style
            }),
    ]
    .width(Length::Fill);

    container(
        column![
            title,
            fan_selector,
            canvas,
            presets,
            curve_toggle,
            apply_row
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

// Button style helpers

fn active_button_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(colors::ACCENT_CYAN.into()),
        text_color: colors::BG_BASE,
        border: iced::Border {
            color: colors::ACCENT_CYAN,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn inactive_button_style(_theme: &Theme, status: button::Status) -> button::Style {
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
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

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
