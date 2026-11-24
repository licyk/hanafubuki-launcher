use serde_json::{Map, Value};

use crate::internal::hanafubuki_config::model::presentation::{
    MAX_BLUR_RADIUS, MAX_SURFACE_OPACITY, MIN_BLUR_RADIUS, MIN_SURFACE_OPACITY,
    normalize_theme_color,
};

use super::field::{FieldReader, within};
use super::{AppSettings, PersistedSettingsIssue};

pub(super) const FIELDS: &[&str] = &[
    "theme",
    "themeColor",
    "backgroundImageEnabled",
    "backgroundRandomEnabled",
    "backgroundSurfaceOpacity",
    "backgroundBlurRadius",
    "backgroundImageFit",
    "backgroundImagePosition",
];

pub(super) fn decode(
    object: &Map<String, Value>,
    defaults: &AppSettings,
    settings: &mut AppSettings,
    issues: &mut Vec<PersistedSettingsIssue>,
) {
    let mut fields = FieldReader::new(object, issues);
    settings.theme = fields.read("theme", defaults.theme);
    // A seed the interface cannot derive an accent from would leave every
    // control unstyled, so only a seed that normalizes is kept.
    settings.theme_color = fields.read_checked(
        "themeColor",
        defaults.theme_color.clone(),
        |value: String| normalize_theme_color(&value),
    );
    settings.background_image_enabled =
        fields.read("backgroundImageEnabled", defaults.background_image_enabled);
    settings.background_random_enabled = fields.read(
        "backgroundRandomEnabled",
        defaults.background_random_enabled,
    );
    settings.background_surface_opacity = fields.read_checked(
        "backgroundSurfaceOpacity",
        defaults.background_surface_opacity,
        within(MIN_SURFACE_OPACITY..=MAX_SURFACE_OPACITY),
    );
    settings.background_blur_radius = fields.read_checked(
        "backgroundBlurRadius",
        defaults.background_blur_radius,
        within(MIN_BLUR_RADIUS..=MAX_BLUR_RADIUS),
    );
    settings.background_image_fit =
        fields.read("backgroundImageFit", defaults.background_image_fit);
    settings.background_image_position = fields.read(
        "backgroundImagePosition",
        defaults.background_image_position,
    );
}
