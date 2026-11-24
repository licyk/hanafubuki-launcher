//! Persisted presentation values owned by the settings contract.
//!
//! Appearance assets consume these values, but the configuration repository
//! must not depend on the appearance service that stores and serves image
//! payloads.

use super::choice::setting_choice;

/// Resolved presentation theme. `System` follows the operating system and is
/// resolved in the frontend against `prefers-color-scheme`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

setting_choice!(ThemePreference, "theme", {
    System => "system",
    Light => "light",
    Dark => "dark",
});

/// How the wallpaper fills the shell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundFit {
    #[default]
    Cover,
    Contain,
    Stretch,
}

setting_choice!(BackgroundFit, "backgroundImageFit", {
    Cover => "cover",
    Contain => "contain",
    Stretch => "stretch",
});

/// Which part of the image stays visible when the selected fit leaves an
/// overflowing or free axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundPosition {
    #[default]
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

setting_choice!(BackgroundPosition, "backgroundImagePosition", {
    Center => "center",
    Top => "top",
    Bottom => "bottom",
    Left => "left",
    Right => "right",
    TopLeft => "topLeft",
    TopRight => "topRight",
    BottomLeft => "bottomLeft",
    BottomRight => "bottomRight",
});

pub const MIN_SURFACE_OPACITY: u8 = 40;
pub const MAX_SURFACE_OPACITY: u8 = 100;
pub const DEFAULT_SURFACE_OPACITY: u8 = 88;

/// Frosted-glass blur radius (CSS px) applied behind translucent surfaces
/// while a background image is visible. 0 disables the effect entirely.
pub const MIN_BLUR_RADIUS: u8 = 0;
pub const MAX_BLUR_RADIUS: u8 = 32;
pub const DEFAULT_BLUR_RADIUS: u8 = 12;

/// Seed colour the whole interface accent is derived from.
///
/// Stored as one `#rrggbb` value and nothing more. Every shade the interface
/// actually paints — the accent foreground, the filled-control background, its
/// hover, the soft tint, and the text that sits on top of it — is derived from
/// this seed per theme by the frontend, which is the only side that knows the
/// surface colours it has to hold contrast against. Storing the derived shades
/// would freeze them against whichever theme happened to be active when the
/// user picked, so the backend deliberately keeps only the choice.
pub const DEFAULT_THEME_COLOR: &str = "#f2a2b8";

/// Validate a theme colour seed, normalising it to lowercase `#rrggbb`.
///
/// Only full six-digit hex is accepted. Shorthand and alpha are rejected rather
/// than expanded: the seed is round-tripped into a colour input and compared
/// against the presets, and two spellings of one colour would make the picker
/// disagree with itself about which preset is selected.
pub fn normalize_theme_color(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let digits = trimmed
        .strip_prefix('#')
        .ok_or_else(|| format!("themeColor must start with '#': {trimmed}"))?;
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "themeColor must be a six-digit hex colour such as {DEFAULT_THEME_COLOR}: {trimmed}"
        ));
    }
    Ok(format!("#{}", digits.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests;
