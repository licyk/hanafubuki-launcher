//! The theme colour seed contract: what the picker may store, and the one
//! spelling each colour has once stored.

use super::*;

#[test]
fn the_default_seed_is_a_value_the_validator_accepts() {
    assert_eq!(
        normalize_theme_color(DEFAULT_THEME_COLOR).unwrap(),
        DEFAULT_THEME_COLOR
    );
}

#[test]
fn normalisation_is_idempotent() {
    let once = normalize_theme_color("#AbCdEf").unwrap();
    assert_eq!(once, "#abcdef");
    assert_eq!(normalize_theme_color(&once).unwrap(), once);
}

#[test]
fn only_full_six_digit_hex_is_accepted() {
    for accepted in ["#000000", "#ffffff", "#F2A2B8", " #123abc "] {
        normalize_theme_color(accepted).unwrap_or_else(|e| panic!("{accepted}: {e}"));
    }
    for refused in [
        "", "#", "#fff", "#12345", "#1234567", "123456", "#12345g", "pink",
    ] {
        assert!(
            normalize_theme_color(refused).is_err(),
            "{refused} should be refused"
        );
    }
}
