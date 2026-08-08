//! Midnight aurora theme and color definitions for nvctl-gui.
//!
//! Keeps the original glossy neon character while using a more restrained,
//! cohesive accent palette for long-running control-panel use.

use iced::theme::{Custom, Palette};
use iced::Theme;
use std::sync::Arc;

/// Background color layers - Modern glass aesthetic
pub mod colors {
    use iced::Color;

    // ═══════════════════════════════════════════════════════════════════════════
    // BACKGROUND LAYERS - Deep modern with glass feel
    // ═══════════════════════════════════════════════════════════════════════════

    /// Deepest background - midnight blue-black (#06090f)
    pub const BG_BASE: Color = Color::from_rgb(0.024, 0.035, 0.059);

    /// Card/panel background - cool glass base (#0c1220)
    pub const BG_SURFACE: Color = Color::from_rgb(0.047, 0.071, 0.125);

    /// Elevated elements - blue-slate lift (#151d2e)
    pub const BG_ELEVATED: Color = Color::from_rgb(0.082, 0.114, 0.18);

    /// Overlay/hover - focused indigo (#1f2940)
    pub const BG_OVERLAY: Color = Color::from_rgb(0.12, 0.16, 0.25);

    /// Card highlight - selected blue-slate (#263554)
    #[allow(dead_code)]
    pub const BG_HIGHLIGHT: Color = Color::from_rgb(0.15, 0.21, 0.33);

    // ═══════════════════════════════════════════════════════════════════════════
    // GLASS EFFECT COLORS - For glossy reflections and highlights
    // ═══════════════════════════════════════════════════════════════════════════

    /// Glass highlight - Top edge shine
    pub const GLASS_HIGHLIGHT: Color = Color::from_rgba(0.80, 0.91, 1.0, 0.10);

    /// Glass shine - Bright reflection spot
    pub const GLASS_SHINE: Color = Color::from_rgba(0.78, 0.91, 1.0, 0.18);

    /// Glass gradient top - For glossy gradient
    #[allow(dead_code)]
    pub const GLASS_TOP: Color = Color::from_rgba(0.72, 0.86, 1.0, 0.07);

    /// Glass border - Subtle luminous border
    pub const GLASS_BORDER: Color = Color::from_rgba(0.34, 0.58, 0.82, 0.32);

    // ═══════════════════════════════════════════════════════════════════════════
    // TEXT COLORS - High contrast for readability
    // ═══════════════════════════════════════════════════════════════════════════

    /// Primary text - cool white (#edf5ff)
    pub const TEXT_PRIMARY: Color = Color::from_rgb(0.93, 0.96, 1.0);

    /// Secondary text - blue-gray (#9eadcc)
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.62, 0.68, 0.80);

    /// Muted/disabled text - readable slate (#63758f)
    pub const TEXT_MUTED: Color = Color::from_rgb(0.39, 0.46, 0.56);

    /// Label text - subdued slate (#47546b)
    #[allow(dead_code)]
    pub const TEXT_LABEL: Color = Color::from_rgb(0.28, 0.33, 0.42);

    // ═══════════════════════════════════════════════════════════════════════════
    // VIBRANT ACCENT COLORS - Bold, colorful, and glossy
    // ═══════════════════════════════════════════════════════════════════════════

    /// Primary accent - arctic cyan (#2ec5ff)
    pub const ACCENT_CYAN: Color = Color::from_rgb(0.18, 0.77, 1.0);

    /// Cyan glow variant - focused highlight
    pub const ACCENT_CYAN_BRIGHT: Color = Color::from_rgb(0.55, 0.90, 1.0);

    /// Cyan dim - selected backgrounds and subtle effects
    pub const ACCENT_CYAN_DIM: Color = Color::from_rgb(0.04, 0.27, 0.40);

    /// Success/positive - vivid mint (#33ea9e)
    pub const ACCENT_GREEN: Color = Color::from_rgb(0.20, 0.92, 0.62);

    /// Green bright - For glossy highlights
    pub const ACCENT_GREEN_BRIGHT: Color = Color::from_rgb(0.55, 1.0, 0.80);

    /// Green dim - For backgrounds
    pub const ACCENT_GREEN_DIM: Color = Color::from_rgb(0.04, 0.32, 0.22);

    /// Warning - warm amber (#ffb454)
    pub const ACCENT_ORANGE: Color = Color::from_rgb(1.0, 0.706, 0.33);

    /// Orange bright - For glossy highlights
    pub const ACCENT_ORANGE_BRIGHT: Color = Color::from_rgb(1.0, 0.82, 0.45);

    /// Orange dim - For backgrounds
    pub const ACCENT_ORANGE_DIM: Color = Color::from_rgb(0.35, 0.20, 0.02);

    /// Error/critical - coral red (#ff5c7a)
    pub const ACCENT_RED: Color = Color::from_rgb(1.0, 0.36, 0.48);

    /// Red bright - For glossy highlights
    pub const ACCENT_RED_BRIGHT: Color = Color::from_rgb(1.0, 0.60, 0.68);

    /// Red dim - For backgrounds
    pub const ACCENT_RED_DIM: Color = Color::from_rgb(0.36, 0.05, 0.13);

    /// Alternative accent - orchid (#d98cff)
    pub const ACCENT_MAGENTA: Color = Color::from_rgb(0.85, 0.55, 1.0);

    /// Magenta bright - For glossy effects
    #[allow(dead_code)]
    pub const ACCENT_MAGENTA_BRIGHT: Color = Color::from_rgb(1.0, 0.5, 0.8);

    /// Magenta dim - For subtle effects
    #[allow(dead_code)]
    pub const ACCENT_MAGENTA_DIM: Color = Color::from_rgb(0.45, 0.0, 0.3);

    /// Alternative accent - periwinkle (#9d8cff)
    pub const ACCENT_PURPLE: Color = Color::from_rgb(0.62, 0.55, 1.0);

    /// Purple bright - For glossy effects
    #[allow(dead_code)]
    pub const ACCENT_PURPLE_BRIGHT: Color = Color::from_rgb(0.75, 0.6, 1.0);

    /// Yellow-green transitional color - soft lime (#b8ef65)
    pub const ACCENT_LIME: Color = Color::from_rgb(0.72, 0.94, 0.40);

    /// Lime bright - For glossy effects
    #[allow(dead_code)]
    pub const ACCENT_LIME_BRIGHT: Color = Color::from_rgb(0.7, 1.0, 0.45);

    /// Sky blue - Additional colorful accent (#33b6ff)
    #[allow(dead_code)]
    pub const ACCENT_SKY: Color = Color::from_rgb(0.20, 0.71, 1.0);

    /// Electric Blue - Deep vibrant blue (#3d8bff)
    #[allow(dead_code)]
    pub const ACCENT_BLUE: Color = Color::from_rgb(0.24, 0.55, 1.0);

    /// Gold accent - Premium feel (#ffd700)
    #[allow(dead_code)]
    pub const ACCENT_GOLD: Color = Color::from_rgb(1.0, 0.843, 0.0);

    // ═══════════════════════════════════════════════════════════════════════════
    // DYNAMIC COLOR FUNCTIONS - Temperature/Speed/Power based coloring
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get color based on temperature with smooth gradient feel
    ///
    /// - Cold (< 35°C): Electric Cyan
    /// - Cool (35-50°C): Neon Green
    /// - Warm (50-65°C): Lime Yellow-green
    /// - Hot (65-80°C): Neon Orange
    /// - Critical (> 80°C): Neon Red
    #[allow(dead_code)]
    pub fn temp_color(celsius: i32) -> Color {
        match celsius {
            t if t < 35 => ACCENT_CYAN,
            t if t < 50 => ACCENT_GREEN,
            t if t < 65 => ACCENT_LIME,
            t if t < 80 => ACCENT_ORANGE,
            _ => ACCENT_RED,
        }
    }

    /// Get dim variant of temperature color (for backgrounds/glows)
    #[allow(dead_code)]
    pub fn temp_color_dim(celsius: i32) -> Color {
        match celsius {
            t if t < 35 => ACCENT_CYAN_DIM,
            t if t < 50 => ACCENT_GREEN_DIM,
            t if t < 65 => Color::from_rgb(0.27, 0.5, 0.0),
            t if t < 80 => ACCENT_ORANGE_DIM,
            _ => ACCENT_RED_DIM,
        }
    }

    /// Get bright variant of temperature color (for highlights)
    #[allow(dead_code)]
    pub fn temp_color_bright(celsius: i32) -> Color {
        match celsius {
            t if t < 35 => ACCENT_CYAN_BRIGHT,
            t if t < 50 => ACCENT_GREEN_BRIGHT,
            t if t < 65 => Color::from_rgb(0.7, 1.0, 0.4),
            t if t < 80 => ACCENT_ORANGE_BRIGHT,
            _ => ACCENT_RED_BRIGHT,
        }
    }

    /// Get color based on fan speed percentage
    #[allow(dead_code)]
    pub fn fan_color(percentage: u8) -> Color {
        match percentage {
            p if p < 25 => ACCENT_CYAN,
            p if p < 45 => ACCENT_GREEN,
            p if p < 65 => ACCENT_LIME,
            p if p < 85 => ACCENT_ORANGE,
            _ => ACCENT_RED,
        }
    }

    /// Get dim variant of fan color
    #[allow(dead_code)]
    pub fn fan_color_dim(percentage: u8) -> Color {
        match percentage {
            p if p < 25 => ACCENT_CYAN_DIM,
            p if p < 45 => ACCENT_GREEN_DIM,
            p if p < 65 => Color::from_rgb(0.27, 0.5, 0.0),
            p if p < 85 => ACCENT_ORANGE_DIM,
            _ => ACCENT_RED_DIM,
        }
    }

    /// Get color based on power usage ratio (0.0-1.0)
    #[allow(dead_code)]
    pub fn power_color(ratio: f32) -> Color {
        match ratio {
            r if r < 0.4 => ACCENT_GREEN,
            r if r < 0.6 => ACCENT_LIME,
            r if r < 0.8 => ACCENT_ORANGE,
            _ => ACCENT_RED,
        }
    }

    /// Get dim variant of power color
    #[allow(dead_code)]
    pub fn power_color_dim(ratio: f32) -> Color {
        match ratio {
            r if r < 0.4 => ACCENT_GREEN_DIM,
            r if r < 0.6 => Color::from_rgb(0.27, 0.5, 0.0),
            r if r < 0.8 => ACCENT_ORANGE_DIM,
            _ => ACCENT_RED_DIM,
        }
    }

    /// Create a color with modified alpha
    pub fn with_alpha(color: Color, alpha: f32) -> Color {
        Color { a: alpha, ..color }
    }

    /// Interpolate between two colors
    pub fn lerp(from: Color, to: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        Color::from_rgba(
            from.r + (to.r - from.r) * t,
            from.g + (to.g - from.g) * t,
            from.b + (to.b - from.b) * t,
            from.a + (to.a - from.a) * t,
        )
    }

    /// Create a glossy highlight variant of a color (lighter, slightly desaturated)
    pub fn glossy_highlight(color: Color) -> Color {
        // Blend toward white with reduced saturation for glossy effect
        lerp(color, Color::WHITE, 0.4)
    }

    /// Create a glossy version of a color (adds brightness)
    #[allow(dead_code)]
    pub fn glossy(color: Color, intensity: f32) -> Color {
        let intensity = intensity.clamp(0.0, 1.0);
        Color::from_rgba(
            (color.r + intensity * 0.3).min(1.0),
            (color.g + intensity * 0.3).min(1.0),
            (color.b + intensity * 0.3).min(1.0),
            color.a,
        )
    }

    /// Get a smooth gradient color between two temp ranges
    pub fn temp_gradient(celsius: i32) -> Color {
        let t = (celsius as f32 / 100.0).clamp(0.0, 1.0);
        if t < 0.35 {
            lerp(ACCENT_CYAN, ACCENT_GREEN, t / 0.35)
        } else if t < 0.5 {
            lerp(ACCENT_GREEN, ACCENT_LIME, (t - 0.35) / 0.15)
        } else if t < 0.7 {
            lerp(ACCENT_LIME, ACCENT_ORANGE, (t - 0.5) / 0.2)
        } else {
            lerp(ACCENT_ORANGE, ACCENT_RED, (t - 0.7) / 0.3)
        }
    }

    /// Rainbow color based on position (0.0 to 1.0)
    pub fn rainbow(t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        if t < 0.17 {
            lerp(ACCENT_RED, ACCENT_ORANGE, t / 0.17)
        } else if t < 0.33 {
            lerp(ACCENT_ORANGE, ACCENT_LIME, (t - 0.17) / 0.16)
        } else if t < 0.5 {
            lerp(ACCENT_LIME, ACCENT_GREEN, (t - 0.33) / 0.17)
        } else if t < 0.67 {
            lerp(ACCENT_GREEN, ACCENT_CYAN, (t - 0.5) / 0.17)
        } else if t < 0.83 {
            lerp(ACCENT_CYAN, ACCENT_PURPLE, (t - 0.67) / 0.16)
        } else {
            lerp(ACCENT_PURPLE, ACCENT_MAGENTA, (t - 0.83) / 0.17)
        }
    }
}

/// Create the custom nvctl dark theme
pub fn nvctl_theme() -> Theme {
    Theme::Custom(Arc::new(Custom::new(
        "nvctl-midnight-aurora".to_string(),
        Palette {
            background: colors::BG_BASE,
            text: colors::TEXT_PRIMARY,
            primary: colors::ACCENT_CYAN,
            success: colors::ACCENT_GREEN,
            danger: colors::ACCENT_RED,
        },
    )))
}

/// Spacing constants - Generous for premium feel
#[allow(dead_code)]
pub mod spacing {
    /// Extra small spacing (4px)
    pub const XS: u16 = 4;
    /// Small spacing (8px)
    pub const SM: u16 = 8;
    /// Medium spacing (16px)
    pub const MD: u16 = 16;
    /// Large spacing (24px)
    pub const LG: u16 = 24;
    /// Extra large spacing (32px)
    pub const XL: u16 = 32;
    /// 2XL spacing (48px)
    pub const XXL: u16 = 48;
}

/// Font sizes - Bold hierarchy
#[allow(dead_code)]
pub mod font_size {
    /// Extra small (10px) - Labels
    pub const XS: u16 = 10;
    /// Small (12px) - Captions
    pub const SM: u16 = 12;
    /// Base (14px) - Body text
    pub const BASE: u16 = 14;
    /// Large (16px) - Emphasis
    pub const LG: u16 = 16;
    /// Extra large (18px) - Subheadings
    pub const XL: u16 = 18;
    /// 2XL (22px) - Headings
    pub const XXL: u16 = 22;
    /// 3XL (28px) - Large headings
    pub const XXXL: u16 = 28;
    /// Display (36px) - Hero numbers in gauges
    pub const DISPLAY: u16 = 36;
    /// Hero (48px) - Extra large display numbers
    pub const HERO: u16 = 48;
}

/// Border radius constants
#[allow(dead_code)]
pub mod radius {
    /// Small radius (4px)
    pub const SM: f32 = 4.0;
    /// Medium radius (8px)
    pub const MD: f32 = 8.0;
    /// Large radius (12px)
    pub const LG: f32 = 12.0;
    /// Extra large radius (16px)
    pub const XL: f32 = 16.0;
    /// Round (9999px)
    pub const ROUND: f32 = 9999.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_color_ranges() {
        // Cold
        assert_eq!(colors::temp_color(30), colors::ACCENT_CYAN);
        // Cool
        assert_eq!(colors::temp_color(45), colors::ACCENT_GREEN);
        // Hot
        assert_eq!(colors::temp_color(75), colors::ACCENT_ORANGE);
        // Critical
        assert_eq!(colors::temp_color(85), colors::ACCENT_RED);
    }

    #[test]
    fn test_nvctl_theme_creation() {
        let theme = nvctl_theme();
        assert!(matches!(theme, Theme::Custom(_)));
    }

    #[test]
    fn test_with_alpha() {
        let color = colors::with_alpha(colors::ACCENT_CYAN, 0.5);
        assert!((color.a - 0.5).abs() < 0.001);
    }
}
