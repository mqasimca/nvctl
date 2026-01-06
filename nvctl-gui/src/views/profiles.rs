//! Profiles view
//!
//! Profile management UI for saving, loading, and managing GPU configuration profiles.

use crate::message::{Message, ProfileMessage};
use crate::services::Profile;
use crate::state::AppState;
use crate::theme::{colors, font_size, spacing};

use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length, Theme};

/// Render the profiles view
pub fn view_profiles(state: &AppState) -> Element<'_, Message> {
    let header = view_header(state);
    let profile_list = view_profile_list(state);
    let actions = view_actions(state);

    let main_content = column![header, profile_list, actions]
        .spacing(spacing::LG)
        .padding(spacing::LG)
        .width(Length::Fill)
        .height(Length::Fill);

    // Show dialog overlays
    if let Some(ref profile_name) = state.pending_delete_profile {
        let overlay = view_delete_confirmation(profile_name);
        iced::widget::stack![main_content, overlay].into()
    } else if state.editing_profile.is_some() {
        let overlay = view_edit_dialog(state);
        iced::widget::stack![main_content, overlay].into()
    } else {
        main_content.into()
    }
}

/// Profiles header
fn view_header(state: &AppState) -> Element<'_, Message> {
    let title = text("Profiles")
        .size(font_size::XXL)
        .color(colors::TEXT_PRIMARY);

    let count_text = text(format!("{} profiles", state.profiles.len()))
        .size(font_size::BASE)
        .color(colors::TEXT_MUTED);

    row![title, horizontal_space(), count_text]
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// Profile list
fn view_profile_list(state: &AppState) -> Element<'_, Message> {
    let title = text("Saved Profiles")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let profiles: Vec<Element<'_, Message>> = if state.profiles.is_empty() {
        vec![
            text("No profiles saved yet.")
                .size(font_size::BASE)
                .color(colors::TEXT_MUTED)
                .into(),
            text("Create a new profile to save your current GPU settings.")
                .size(font_size::SM)
                .color(colors::TEXT_MUTED)
                .into(),
        ]
    } else {
        state
            .profiles
            .iter()
            .map(|profile| view_profile_card(profile, state))
            .collect()
    };

    let profile_column = iced::widget::Column::with_children(profiles).spacing(spacing::SM);

    let scrollable_list = scrollable(profile_column)
        .height(Length::Fill)
        .width(Length::Fill);

    container(column![title, scrollable_list].spacing(spacing::MD))
        .padding(spacing::MD)
        .width(Length::Fill)
        .height(Length::Fill)
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

/// Single profile card
fn view_profile_card<'a>(profile: &'a Profile, state: &'a AppState) -> Element<'a, Message> {
    let is_active = state.active_profile.as_ref() == Some(&profile.name);
    let is_default = profile.is_default;

    let name_color = if is_active {
        colors::ACCENT_CYAN
    } else {
        colors::TEXT_PRIMARY
    };

    let name = text(&profile.name).size(font_size::BASE).color(name_color);

    let badges: Element<'_, Message> = {
        let mut badge_row = row![].spacing(spacing::XS);

        if is_default {
            badge_row = badge_row.push(
                container(text("Default").size(font_size::XS).color(colors::BG_BASE))
                    .padding([2, 6])
                    .style(|_theme| container::Style {
                        background: Some(colors::ACCENT_GREEN.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            );
        }

        if is_active {
            badge_row = badge_row.push(
                container(text("Active").size(font_size::XS).color(colors::BG_BASE))
                    .padding([2, 6])
                    .style(|_theme| container::Style {
                        background: Some(colors::ACCENT_CYAN.into()),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            );
        }

        badge_row.into()
    };

    let description: Element<'_, Message> = if let Some(ref desc) = profile.description {
        text(desc)
            .size(font_size::SM)
            .color(colors::TEXT_SECONDARY)
            .into()
    } else {
        text("No description")
            .size(font_size::SM)
            .color(colors::TEXT_MUTED)
            .into()
    };

    let settings_info = {
        let gpu_count = profile.gpu_settings.len();
        let info = if gpu_count == 0 {
            "No GPU settings".to_string()
        } else if gpu_count == 1 {
            "1 GPU configured".to_string()
        } else {
            format!("{} GPUs configured", gpu_count)
        };
        text(info).size(font_size::XS).color(colors::TEXT_MUTED)
    };

    let name_row = row![name, badges]
        .spacing(spacing::SM)
        .align_y(Alignment::Center);

    let info_column = column![name_row, description, settings_info].spacing(spacing::XS);

    // Action buttons
    let apply_btn = button(text("Apply").size(font_size::SM))
        .on_press(Message::Profile(ProfileMessage::Selected(
            profile.name.clone(),
        )))
        .padding([spacing::XS, spacing::SM])
        .style(if is_active {
            active_button_style
        } else {
            primary_button_style
        });

    let edit_btn = button(text("Edit").size(font_size::SM))
        .on_press(Message::Profile(ProfileMessage::StartEdit(
            profile.name.clone(),
        )))
        .padding([spacing::XS, spacing::SM])
        .style(secondary_button_style);

    let delete_btn = button(text("Delete").size(font_size::SM))
        .on_press(Message::Profile(ProfileMessage::RequestDelete(
            profile.name.clone(),
        )))
        .padding([spacing::XS, spacing::SM])
        .style(danger_button_style);

    let buttons = row![apply_btn, edit_btn, delete_btn]
        .spacing(spacing::XS)
        .align_y(Alignment::Center);

    let card_content = row![info_column, horizontal_space(), buttons]
        .align_y(Alignment::Center)
        .width(Length::Fill);

    container(card_content)
        .padding(spacing::MD)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(
                if is_active {
                    colors::BG_ELEVATED
                } else {
                    colors::BG_SURFACE
                }
                .into(),
            ),
            border: iced::Border {
                color: if is_active {
                    colors::ACCENT_CYAN
                } else {
                    colors::BG_OVERLAY
                },
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Actions section (create new profile)
fn view_actions(state: &AppState) -> Element<'_, Message> {
    let title = text("Create New Profile")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let name_input = text_input("Profile name...", &state.new_profile_name)
        .on_input(|s| Message::Profile(ProfileMessage::NameInputChanged(s)))
        .padding(spacing::SM)
        .size(font_size::BASE)
        .width(Length::Fill);

    let save_enabled = !state.new_profile_name.trim().is_empty()
        && !state
            .profiles
            .iter()
            .any(|p| p.name == state.new_profile_name.trim());

    let save_btn = if save_enabled {
        button(text("Save Current Settings").size(font_size::BASE))
            .on_press(Message::Profile(ProfileMessage::SaveCurrent(
                state.new_profile_name.trim().to_string(),
            )))
            .padding([spacing::SM, spacing::MD])
            .style(primary_button_style)
    } else {
        button(text("Save Current Settings").size(font_size::BASE))
            .padding([spacing::SM, spacing::MD])
            .style(disabled_button_style)
    };

    let input_row = row![name_input, save_btn]
        .spacing(spacing::MD)
        .align_y(Alignment::Center);

    let hint = if state.new_profile_name.trim().is_empty() {
        text("Enter a name for your new profile")
            .size(font_size::XS)
            .color(colors::TEXT_MUTED)
    } else if state
        .profiles
        .iter()
        .any(|p| p.name == state.new_profile_name.trim())
    {
        text("A profile with this name already exists")
            .size(font_size::XS)
            .color(colors::ACCENT_ORANGE)
    } else {
        text("This will save your current fan curve and power settings")
            .size(font_size::XS)
            .color(colors::TEXT_MUTED)
    };

    container(column![title, input_row, hint].spacing(spacing::MD))
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

/// Edit profile dialog overlay
fn view_edit_dialog(state: &AppState) -> Element<'_, Message> {
    let title = text("Rename Profile")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let name_input = text_input("Profile name...", &state.edit_profile_name)
        .on_input(|s| Message::Profile(ProfileMessage::EditNameChanged(s)))
        .padding(spacing::SM)
        .size(font_size::BASE)
        .width(Length::Fill);

    let save_enabled = !state.edit_profile_name.trim().is_empty()
        && state.editing_profile.as_ref() != Some(&state.edit_profile_name)
        && !state
            .profiles
            .iter()
            .any(|p| p.name == state.edit_profile_name.trim());

    let cancel_btn = button(text("Cancel").size(font_size::BASE))
        .on_press(Message::Profile(ProfileMessage::CancelEdit))
        .padding([spacing::SM, spacing::MD])
        .style(secondary_button_style);

    let save_btn = if save_enabled {
        button(text("Save").size(font_size::BASE))
            .on_press(Message::Profile(ProfileMessage::ConfirmEdit))
            .padding([spacing::SM, spacing::MD])
            .style(primary_button_style)
    } else {
        button(text("Save").size(font_size::BASE))
            .padding([spacing::SM, spacing::MD])
            .style(disabled_button_style)
    };

    let buttons = row![cancel_btn, save_btn]
        .spacing(spacing::MD)
        .align_y(Alignment::Center);

    let hint = if state.edit_profile_name.trim().is_empty() {
        text("Enter a new name for the profile")
            .size(font_size::XS)
            .color(colors::TEXT_MUTED)
    } else if state.profiles.iter().any(|p| {
        p.name == state.edit_profile_name.trim() && state.editing_profile.as_ref() != Some(&p.name)
    }) {
        text("A profile with this name already exists")
            .size(font_size::XS)
            .color(colors::ACCENT_ORANGE)
    } else {
        text("").size(font_size::XS).color(colors::TEXT_MUTED)
    };

    let dialog_content = column![title, name_input, hint, buttons]
        .spacing(spacing::MD)
        .align_x(Alignment::Center);

    let dialog = container(dialog_content)
        .padding(spacing::LG)
        .width(Length::Fixed(400.0))
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::BG_OVERLAY,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        });

    // Backdrop
    container(
        container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme| container::Style {
        background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5).into()),
        ..Default::default()
    })
    .into()
}

/// Delete confirmation dialog overlay
fn view_delete_confirmation(profile_name: &str) -> Element<'_, Message> {
    let title = text("Delete Profile?")
        .size(font_size::LG)
        .color(colors::TEXT_PRIMARY);

    let message = text(format!(
        "Are you sure you want to delete \"{}\"? This action cannot be undone.",
        profile_name
    ))
    .size(font_size::BASE)
    .color(colors::TEXT_SECONDARY);

    let cancel_btn = button(text("Cancel").size(font_size::BASE))
        .on_press(Message::Profile(ProfileMessage::CancelDelete))
        .padding([spacing::SM, spacing::MD])
        .style(secondary_button_style);

    let delete_btn = button(text("Delete").size(font_size::BASE))
        .on_press(Message::Profile(ProfileMessage::ConfirmDelete))
        .padding([spacing::SM, spacing::MD])
        .style(danger_button_style);

    let buttons = row![cancel_btn, delete_btn]
        .spacing(spacing::MD)
        .align_y(Alignment::Center);

    let dialog_content = column![title, message, buttons]
        .spacing(spacing::MD)
        .align_x(Alignment::Center);

    let dialog = container(dialog_content)
        .padding(spacing::LG)
        .width(Length::Fixed(400.0))
        .style(|_theme| container::Style {
            background: Some(colors::BG_SURFACE.into()),
            border: iced::Border {
                color: colors::BG_OVERLAY,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        });

    // Backdrop
    container(
        container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme| container::Style {
        background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5).into()),
        ..Default::default()
    })
    .into()
}

// Button styles

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
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn active_button_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(colors::BG_ELEVATED.into()),
        text_color: colors::ACCENT_CYAN,
        border: iced::Border {
            color: colors::ACCENT_CYAN,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn danger_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => colors::ACCENT_RED,
        _ => colors::BG_ELEVATED,
    };
    let text_color = match status {
        button::Status::Hovered => colors::BG_BASE,
        _ => colors::ACCENT_RED,
    };
    button::Style {
        background: Some(bg.into()),
        text_color,
        border: iced::Border {
            color: colors::ACCENT_RED,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn disabled_button_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(colors::BG_ELEVATED.into()),
        text_color: colors::TEXT_MUTED,
        border: iced::Border {
            color: colors::BG_OVERLAY,
            width: 1.0,
            radius: 4.0.into(),
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
        text_color: colors::TEXT_PRIMARY,
        border: iced::Border {
            color: colors::BG_OVERLAY,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
