use app_web::core;
use app_web::events::keymap::{mode_scale_for_digit, root_midi_for_key};

#[test]
fn root_midi_for_key_valid_keys() {
    let cases = [
        ("a", 69),
        ("b", 71),
        ("c", 60),
        ("d", 62),
        ("e", 64),
        ("f", 65),
        ("g", 67),
    ];
    for (key, midi) in cases {
        assert_eq!(root_midi_for_key(key), Some(midi));
        assert_eq!(root_midi_for_key(&key.to_ascii_uppercase()), Some(midi));
    }
}

#[test]
fn root_midi_for_key_invalid_keys() {
    for key in ["h", "z", "", "1", "0", "notakey", " "] {
        assert_eq!(root_midi_for_key(key), None);
    }
}

#[test]
fn mode_scale_for_digit_valid_digits() {
    let cases = [
        ("1", core::IONIAN),
        ("2", core::DORIAN),
        ("3", core::PHRYGIAN),
        ("4", core::LYDIAN),
        ("5", core::MIXOLYDIAN),
        ("6", core::AEOLIAN),
        ("7", core::LOCRIAN),
        ("8", core::TET19_PENTATONIC),
        ("9", core::TET24_PENTATONIC),
        ("0", core::TET31_PENTATONIC),
    ];
    for (digit, expected) in cases {
        assert_eq!(mode_scale_for_digit(digit), Some(expected));
    }
}

#[test]
fn mode_scale_for_digit_invalid_keys() {
    for key in ["", "a", "-", "10", "Digit1"] {
        assert_eq!(mode_scale_for_digit(key), None);
    }
}

#[test]
fn diatonic_mode_scales_are_well_formed() {
    for digit in ["1", "2", "3", "4", "5", "6", "7"] {
        let scale = mode_scale_for_digit(digit).unwrap();
        assert_eq!(scale.len(), 8, "digit {digit} should map to a 7-note mode");
        assert!((scale[0] - 0.0).abs() < 1e-6);
        assert!((scale[scale.len() - 1] - 12.0).abs() < 1e-6);
        for i in 1..scale.len() {
            assert!(scale[i] > scale[i - 1], "mode {digit} must be monotonic");
        }
    }
}

#[test]
fn alternate_tuning_scales_are_well_formed() {
    for digit in ["8", "9", "0"] {
        let scale = mode_scale_for_digit(digit).unwrap();
        assert_eq!(scale.len(), 6, "digit {digit} should map to pentatonic");
        assert!((scale[0] - 0.0).abs() < 1e-6);
        assert!((scale[scale.len() - 1] - 12.0).abs() < 1e-6);
        for i in 1..scale.len() {
            assert!(scale[i] > scale[i - 1], "tuning {digit} must be monotonic");
        }
    }
}
