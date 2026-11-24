use iced::Length;

use super::{LOADING_SLIDER_UNITS, LOADING_TRACK_UNITS, loading_segment, loading_width};

#[test]
fn zero_loading_units_have_fixed_zero_width() {
    assert_eq!(loading_width(0), Length::Fixed(0.0));
    assert_eq!(loading_width(1), Length::FillPortion(1));
}

#[test]
fn loading_segment_enters_slides_and_leaves_the_track() {
    let track = f32::from(LOADING_TRACK_UNITS);
    let slider = f32::from(LOADING_SLIDER_UNITS) / track;
    let entered = slider / (1.0 + slider);
    let leaving = 1.0 / (1.0 + slider);

    assert_eq!(loading_segment(0.0), (0, 0, LOADING_TRACK_UNITS));
    assert_eq!(
        loading_segment(entered),
        (
            0,
            LOADING_SLIDER_UNITS,
            LOADING_TRACK_UNITS - LOADING_SLIDER_UNITS,
        )
    );
    assert_eq!(
        loading_segment(0.5),
        (
            (LOADING_TRACK_UNITS - LOADING_SLIDER_UNITS) / 2,
            LOADING_SLIDER_UNITS,
            (LOADING_TRACK_UNITS - LOADING_SLIDER_UNITS) / 2,
        )
    );
    assert_eq!(
        loading_segment(leaving),
        (
            LOADING_TRACK_UNITS - LOADING_SLIDER_UNITS,
            LOADING_SLIDER_UNITS,
            0,
        )
    );
    assert_eq!(loading_segment(1.0), (LOADING_TRACK_UNITS, 0, 0));
}

#[test]
fn loading_segment_edges_share_one_constant_speed() {
    let track = f32::from(LOADING_TRACK_UNITS);
    let slider = f32::from(LOADING_SLIDER_UNITS) / track;
    let speed = 1.0 + slider;
    let steps = 100;
    let step = 1.0 / steps as f32;
    // Rounding to whole track units can shift either sample by one unit.
    let tolerance = 2.0 / track;

    for sample in 0..steps {
        let position = sample as f32 * step;
        let (left, width, _) = loading_segment(position);
        let (next_left, next_width, _) = loading_segment(position + step);

        let leading = f32::from(left + width) / track;
        let next_leading = f32::from(next_left + next_width) / track;
        if next_left + next_width < LOADING_TRACK_UNITS {
            assert!(
                (next_leading - leading - speed * step).abs() < tolerance,
                "leading edge speed changed at {position}"
            );
        }

        let trailing = f32::from(left) / track;
        let next_trailing = f32::from(next_left) / track;
        if left > 0 {
            assert!(
                (next_trailing - trailing - speed * step).abs() < tolerance,
                "trailing edge speed changed at {position}"
            );
        }
    }
}
