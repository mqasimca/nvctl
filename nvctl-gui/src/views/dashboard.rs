//! Dashboard view
//!
//! Glossy dashboard with vibrant gauges and glassmorphism cards.

use crate::message::Message;
use crate::state::{AppState, GpuState};
use crate::theme::{colors, font_size, spacing};
use crate::widgets::{
    DataSeries, FanGauge, MultiSeriesGraph, PowerBar, TempGauge, UtilGauge, VramBar,
};

use iced::widget::{button, column, container, horizontal_space, row, scrollable, text, Canvas};
use iced::{Alignment, Color, Element, Length, Theme};

/// Render the dashboard view
pub fn view_dashboard(state: &AppState) -> Element<'_, Message> {
    let header = view_header(state);

    let content: Element<'_, Message> = if state.gpus.is_empty() {
        view_no_gpus()
    } else if state.has_multiple_gpus() {
        view_multi_gpu_dashboard(state)
    } else if let Some(gpu) = state.current_gpu() {
        view_gpu_dashboard(state, gpu)
    } else {
        view_no_gpus()
    };

    let layout = column![header, content]
        .spacing(spacing::XL)
        .padding(spacing::LG)
        .width(Length::Fill);

    scrollable(layout).height(Length::Fill).into()
}

/// Dashboard header with GPU selector
fn view_header(state: &AppState) -> Element<'_, Message> {
    let title = text("Dashboard")
        .size(font_size::XXXL)
        .color(colors::TEXT_PRIMARY);

    let gpu_info: Element<'_, Message> = if let Some(gpu) = state.current_gpu() {
        row![text(gpu.info.short_name())
            .size(font_size::LG)
            .color(colors::ACCENT_CYAN),]
        .spacing(spacing::SM)
        .align_y(Alignment::Center)
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

/// View when no GPUs are detected
fn view_no_gpus() -> Element<'static, Message> {
    let icon = text("GPU").size(font_size::HERO).color(colors::TEXT_MUTED);

    let message = text("No NVIDIA GPUs detected")
        .size(font_size::XXL)
        .color(colors::TEXT_SECONDARY);

    let hint = text("Make sure you have NVIDIA drivers installed and a compatible GPU connected.")
        .size(font_size::BASE)
        .color(colors::TEXT_MUTED);

    container(
        column![icon, message, hint]
            .spacing(spacing::MD)
            .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

/// Multi-GPU dashboard
fn view_multi_gpu_dashboard(state: &AppState) -> Element<'_, Message> {
    let summary = state.all_gpus_summary();
    let summary_card = view_multi_gpu_summary(summary, state.linked_gpus);
    let link_toggle = view_link_toggle(state);

    let gpu_cards: Vec<Element<'_, Message>> = state
        .gpus
        .iter()
        .enumerate()
        .map(|(idx, gpu)| view_mini_gpu_card(gpu, idx == state.selected_gpu, idx))
        .collect();

    let cards_row = iced::widget::Row::with_children(gpu_cards)
        .spacing(spacing::MD)
        .width(Length::Fill);

    let details: Element<'_, Message> = if let Some(gpu) = state.current_gpu() {
        view_gpu_dashboard(state, gpu)
    } else {
        text("Select a GPU to view details")
            .color(colors::TEXT_MUTED)
            .into()
    };

    column![summary_card, link_toggle, cards_row, details]
        .spacing(spacing::LG)
        .width(Length::Fill)
        .into()
}

/// Multi-GPU summary card with glossy glass effect
fn view_multi_gpu_summary(
    summary: crate::state::MultiGpuSummary,
    linked: bool,
) -> Element<'static, Message> {
    let title = text("System Overview")
        .size(font_size::XXL)
        .color(colors::TEXT_PRIMARY);

    let gpu_count = text(format!("{} GPUs detected", summary.count))
        .size(font_size::BASE)
        .color(colors::TEXT_SECONDARY);

    let link_status = if linked {
        text("Linked (settings apply to all)")
            .size(font_size::SM)
            .color(colors::ACCENT_CYAN)
    } else {
        text("Independent (settings apply to selected)")
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
    };

    let stats = row![
        view_summary_stat(
            "Max Temp",
            &format!("{}°C", summary.max_temp),
            colors::temp_gradient(summary.max_temp)
        ),
        view_summary_stat(
            "Avg Temp",
            &format!("{}°C", summary.avg_temp),
            colors::temp_gradient(summary.avg_temp)
        ),
        view_summary_stat(
            "Total Power",
            &format!("{}W", summary.total_power),
            colors::ACCENT_GREEN
        ),
    ]
    .spacing(spacing::XL);

    container(column![title, gpu_count, link_status, stats].spacing(spacing::SM))
        .padding(spacing::LG)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::with_alpha(colors::ACCENT_CYAN, 0.5),
                width: 1.5,
                radius: 16.0.into(),
            },
            shadow: iced::Shadow {
                color: colors::with_alpha(colors::ACCENT_CYAN, 0.1),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 20.0,
            },
            ..Default::default()
        })
        .into()
}

/// Summary stat display
fn view_summary_stat<'a>(label: &'static str, value: &str, color: Color) -> Element<'a, Message> {
    column![
        text(label).size(font_size::XS).color(colors::TEXT_MUTED),
        text(value.to_string()).size(font_size::XXL).color(color),
    ]
    .spacing(spacing::XS)
    .into()
}

/// Link toggle button
fn view_link_toggle(state: &AppState) -> Element<'_, Message> {
    let linked = state.linked_gpus;

    let icon = if linked { "🔗" } else { "⛓️‍💥" };
    let label = if linked {
        "GPUs Linked"
    } else {
        "GPUs Independent"
    };

    let btn_style = if linked {
        link_active_style
    } else {
        link_inactive_style
    };

    row![
        button(
            row![
                text(icon).size(font_size::BASE),
                text(label).size(font_size::SM),
            ]
            .spacing(spacing::SM)
            .align_y(Alignment::Center)
        )
        .on_press(Message::LinkedGpusToggled(!linked))
        .padding([spacing::SM, spacing::MD])
        .style(btn_style),
        horizontal_space(),
        text("When linked, fan and power settings apply to all GPUs")
            .size(font_size::XS)
            .color(colors::TEXT_MUTED),
    ]
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// Mini GPU card for multi-GPU overview
fn view_mini_gpu_card(gpu: &GpuState, is_selected: bool, index: usize) -> Element<'_, Message> {
    let border_color = if is_selected {
        colors::ACCENT_CYAN
    } else {
        colors::BG_ELEVATED
    };

    let name = text(gpu.info.short_name())
        .size(font_size::BASE)
        .color(if is_selected {
            colors::TEXT_PRIMARY
        } else {
            colors::TEXT_SECONDARY
        });

    let temp = text(format!("{}°C", gpu.temperature.as_celsius()))
        .size(font_size::XXL)
        .color(colors::temp_color(gpu.temperature.as_celsius()));

    let fan = text(format!("Fan: {}%", gpu.average_fan_speed().unwrap_or(0)))
        .size(font_size::SM)
        .color(colors::TEXT_MUTED);

    let power = text(format!("{}W", gpu.power_usage.as_watts()))
        .size(font_size::SM)
        .color(colors::TEXT_MUTED);

    let card_content = column![name, temp, fan, power]
        .spacing(spacing::XS)
        .align_x(Alignment::Center);

    button(card_content)
        .on_press(Message::GpuSelected(index))
        .padding(spacing::MD)
        .width(Length::FillPortion(1))
        .style(move |theme, status| gpu_card_style(theme, status, is_selected, border_color))
        .into()
}

/// GPU card button style
fn gpu_card_style(
    _theme: &Theme,
    status: button::Status,
    is_selected: bool,
    border_color: Color,
) -> button::Style {
    let bg = match status {
        button::Status::Hovered => colors::BG_OVERLAY,
        _ if is_selected => colors::BG_ELEVATED,
        _ => colors::BG_SURFACE,
    };

    button::Style {
        background: Some(bg.into()),
        text_color: colors::TEXT_PRIMARY,
        border: iced::Border {
            color: border_color,
            width: if is_selected { 2.0 } else { 1.0 },
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}

/// Link toggle button styles
fn link_active_style(_theme: &Theme, status: button::Status) -> button::Style {
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
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn link_inactive_style(_theme: &Theme, status: button::Status) -> button::Style {
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
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

/// Main GPU dashboard with gauges
fn view_gpu_dashboard<'a>(_state: &'a AppState, gpu: &'a GpuState) -> Element<'a, Message> {
    // GPU info card
    let gpu_card = view_gpu_card(gpu);

    // Metrics row with 4 gauges
    let metrics = view_metrics_row(gpu);

    // VRAM usage bar
    let vram = view_vram_bar(gpu);

    // Quick stats with larger values
    let stats = view_quick_stats(gpu);

    // Temperature history graph
    let history = view_temp_history(gpu);

    column![gpu_card, metrics, vram, stats, history]
        .spacing(spacing::LG)
        .width(Length::Fill)
        .into()
}

/// VRAM usage bar with glossy styling
fn view_vram_bar(gpu: &GpuState) -> Element<'_, Message> {
    let vram_widget = VramBar::new(gpu.memory_info);
    let vram_canvas: Element<'_, Message> = Canvas::new(vram_widget)
        .width(Length::Fill)
        .height(Length::Fixed(55.0))
        .into();

    let vram_color = if gpu.memory_info.usage_ratio() > 0.8 {
        colors::ACCENT_RED
    } else if gpu.memory_info.usage_ratio() > 0.5 {
        colors::ACCENT_ORANGE
    } else {
        colors::ACCENT_CYAN
    };

    container(vram_canvas)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::with_alpha(vram_color, 0.2),
                width: 1.0,
                radius: 16.0.into(),
            },
            shadow: iced::Shadow {
                color: colors::with_alpha(vram_color, 0.05),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 16.0,
            },
            ..Default::default()
        })
        .into()
}

/// Performance history graph showing multiple metrics
fn view_temp_history(gpu: &GpuState) -> Element<'_, Message> {
    // Build multi-series graph with temperature, GPU utilization, and power
    let temp_series = DataSeries::new(
        gpu.temp_history.data(),
        "Temp",
        "°C",
        colors::temp_color(gpu.temperature.as_celsius()),
    )
    .range(0.0, 100.0);

    let util_series = DataSeries::new(
        gpu.gpu_util_history.data(),
        "GPU",
        "%",
        colors::ACCENT_PURPLE,
    )
    .range(0.0, 100.0);

    // Normalize power to percentage of limit for better visualization
    let power_max = gpu.power_limit.as_watts().max(1) as f32;
    let power_series =
        DataSeries::new(gpu.power_history.data(), "Power", "W", colors::ACCENT_ORANGE)
            .range(0.0, power_max);

    let graph = MultiSeriesGraph::new("Performance History")
        .add_series(temp_series)
        .add_series(util_series)
        .add_series(power_series);

    let canvas: Element<'_, Message> = Canvas::new(graph)
        .width(Length::Fill)
        .height(Length::Fixed(200.0))
        .into();

    container(canvas)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::with_alpha(colors::ACCENT_CYAN, 0.2),
                width: 1.0,
                radius: 16.0.into(),
            },
            shadow: iced::Shadow {
                color: colors::with_alpha(colors::ACCENT_CYAN, 0.05),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 16.0,
            },
            ..Default::default()
        })
        .into()
}

/// GPU information card with glossy glass effect
fn view_gpu_card(gpu: &GpuState) -> Element<'_, Message> {
    let name = text(&gpu.info.name)
        .size(font_size::XXL)
        .color(colors::TEXT_PRIMARY);

    let driver: Element<'_, Message> = if let Some(ref version) = gpu.info.driver_version {
        text(format!("Driver: {}", version))
            .size(font_size::SM)
            .color(colors::ACCENT_CYAN)
            .into()
    } else {
        text("Driver: Unknown")
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
            .into()
    };

    let fan_count = text(format!("{} fan(s)", gpu.info.fan_count))
        .size(font_size::SM)
        .color(colors::TEXT_SECONDARY);

    let info_col = column![name, driver, fan_count].spacing(spacing::XS);

    // Clock speeds display
    let gpu_clock = text(format!("GPU: {} MHz", gpu.gpu_clock.as_mhz()))
        .size(font_size::SM)
        .color(colors::ACCENT_PURPLE);

    let mem_clock = text(format!("MEM: {} MHz", gpu.mem_clock.as_mhz()))
        .size(font_size::SM)
        .color(colors::ACCENT_BLUE);

    let perf_state = text(format!("{}", gpu.perf_state))
        .size(font_size::SM)
        .color(colors::ACCENT_GREEN);

    let clock_col = column![gpu_clock, mem_clock, perf_state]
        .spacing(spacing::XS)
        .align_x(Alignment::End);

    let content_row = row![info_col, horizontal_space(), clock_col]
        .align_y(Alignment::Center)
        .width(Length::Fill);

    container(content_row)
        .padding(spacing::LG)
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::GLASS_BORDER,
                width: 1.0,
                radius: 16.0.into(),
            },
            shadow: iced::Shadow {
                color: colors::with_alpha(colors::ACCENT_PURPLE, 0.08),
                offset: iced::Vector::new(0.0, 4.0),
                blur_radius: 16.0,
            },
            ..Default::default()
        })
        .into()
}

/// Row of metric gauges - 4 gauges for comprehensive monitoring
fn view_metrics_row(gpu: &GpuState) -> Element<'_, Message> {
    // Temperature gauge
    let temp_widget = TempGauge::new(gpu.temperature, gpu.thresholds);
    let temp_canvas: Element<'_, Message> = Canvas::new(temp_widget)
        .width(Length::Fixed(140.0))
        .height(Length::Fixed(140.0))
        .into();
    let temp_color = colors::temp_color(gpu.temperature.as_celsius());
    let temp_gauge = view_metric_card("TEMPERATURE", temp_canvas, temp_color);

    // Fan speed gauge
    let fan_speed = gpu.average_fan_speed().unwrap_or(0);
    let fan_widget = FanGauge::new(fan_speed);
    let fan_canvas: Element<'_, Message> = Canvas::new(fan_widget)
        .width(Length::Fixed(140.0))
        .height(Length::Fixed(140.0))
        .into();
    let fan_color = colors::fan_color(fan_speed);
    let fan_gauge = view_metric_card("FAN SPEED", fan_canvas, fan_color);

    // Power gauge
    let power_widget = PowerBar::new(gpu.power_usage, gpu.power_limit, gpu.power_constraints);
    let power_canvas: Element<'_, Message> = Canvas::new(power_widget)
        .width(Length::Fixed(140.0))
        .height(Length::Fixed(140.0))
        .into();
    let power_color = colors::power_color(gpu.power_ratio());
    let power_gauge = view_metric_card("POWER", power_canvas, power_color);

    // GPU Utilization gauge
    let util_widget = UtilGauge::new(gpu.utilization);
    let util_canvas: Element<'_, Message> = Canvas::new(util_widget)
        .width(Length::Fixed(140.0))
        .height(Length::Fixed(140.0))
        .into();
    let util_color = colors::ACCENT_PURPLE;
    let util_gauge = view_metric_card("GPU USAGE", util_canvas, util_color);

    row![temp_gauge, fan_gauge, power_gauge, util_gauge]
        .spacing(spacing::MD)
        .width(Length::Fill)
        .into()
}

/// Wrap a widget in a glossy glass metric card
fn view_metric_card<'a>(
    label: &'static str,
    content: Element<'a, Message>,
    accent_color: Color,
) -> Element<'a, Message> {
    let label_text = text(label)
        .size(font_size::XS)
        .color(colors::with_alpha(accent_color, 0.7));

    container(
        column![label_text, content]
            .spacing(spacing::SM)
            .align_x(Alignment::Center),
    )
    .padding(spacing::LG)
    .width(Length::FillPortion(1))
    .style(move |_theme| container::Style {
        background: Some(colors::BG_SURFACE.into()),
        border: iced::Border {
            color: colors::with_alpha(accent_color, 0.2),
            width: 1.0,
            radius: 20.0.into(),
        },
        shadow: iced::Shadow {
            color: colors::with_alpha(accent_color, 0.06),
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 24.0,
        },
        ..Default::default()
    })
    .into()
}

/// Quick stats display with glossy styling
fn view_quick_stats(gpu: &GpuState) -> Element<'_, Message> {
    let temp_stat = view_stat(
        "Temperature",
        &format!("{}°C", gpu.temperature.as_celsius()),
        colors::temp_gradient(gpu.temperature.as_celsius()),
    );

    let fan_stat = if let Some(speed) = gpu.average_fan_speed() {
        view_stat(
            "Fan Speed",
            &format!("{}%", speed),
            colors::rainbow(speed as f32 / 100.0),
        )
    } else {
        view_stat("Fan Speed", "N/A", colors::TEXT_MUTED)
    };

    let power_stat = view_stat(
        "Power",
        &format!(
            "{}W / {}W",
            gpu.power_usage.as_watts(),
            gpu.power_limit.as_watts()
        ),
        colors::lerp(colors::ACCENT_GREEN, colors::ACCENT_RED, gpu.power_ratio()),
    );

    let policy_stat = if gpu.has_manual_fans() {
        view_stat("Fan Policy", "Manual", colors::ACCENT_ORANGE)
    } else {
        view_stat("Fan Policy", "Auto", colors::ACCENT_GREEN)
    };

    let util_stat = view_stat(
        "GPU Load",
        &format!("{}%", gpu.utilization.gpu_percent()),
        colors::ACCENT_PURPLE,
    );

    let mem_util_stat = view_stat(
        "Mem Bandwidth",
        &format!("{}%", gpu.utilization.memory_percent()),
        colors::ACCENT_BLUE,
    );

    container(
        row![
            temp_stat,
            fan_stat,
            power_stat,
            policy_stat,
            util_stat,
            mem_util_stat
        ]
        .spacing(spacing::LG)
        .width(Length::Fill),
    )
    .padding(spacing::LG)
    .width(Length::Fill)
    .style(|_theme| container::Style {
        background: Some(colors::BG_SURFACE.into()),
        border: iced::Border {
            color: colors::GLASS_BORDER,
            width: 1.0,
            radius: 16.0.into(),
        },
        shadow: iced::Shadow {
            color: colors::with_alpha(colors::ACCENT_CYAN, 0.04),
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    })
    .into()
}

/// Single stat display with larger typography
fn view_stat<'a>(label: &'static str, value: &str, color: Color) -> Element<'a, Message> {
    let label_text = text(label).size(font_size::SM).color(colors::TEXT_MUTED);

    let value_text = text(value.to_string()).size(font_size::XL).color(color);

    column![label_text, value_text]
        .spacing(spacing::XS)
        .width(Length::FillPortion(1))
        .into()
}
