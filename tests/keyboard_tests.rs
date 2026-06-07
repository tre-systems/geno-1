use app_web::core;
use app_web::events::keymap::{mode_scale_for_digit, root_midi_for_key};

#[test]
fn root_midi_for_key_valid_keys() {
    let cases = [
        ("a", 69.0),
        ("b", 71.0),
        ("c", 60.0),
        ("d", 62.0),
        ("e", 64.0),
        ("f", 65.0),
        ("g", 67.0),
    ];
    for (key, midi) in cases {
        assert_eq!(root_midi_for_key(key), Some(core::MidiNote(midi)));
        assert_eq!(
            root_midi_for_key(&key.to_ascii_uppercase()),
            Some(core::MidiNote(midi))
        );
    }
}

#[test]
fn root_midi_for_key_invalid_keys() {
    for key in ["h", "z", "", "1", "0", "notakey", " "] {
        assert_eq!(root_midi_for_key(key), None);
    }
}

#[test]
fn command_for_key_maps_discrete_actions() {
    use app_web::core::{Cents, MidiNote};
    use app_web::events::command::{command_for_key, InputCommand};

    assert_eq!(
        command_for_key("c", false),
        Some(InputCommand::SetRoot(MidiNote(60.0)))
    );
    assert_eq!(
        command_for_key("1", false),
        Some(InputCommand::SetScale(core::IONIAN))
    );
    assert_eq!(command_for_key(" ", false), Some(InputCommand::TogglePause));
    assert_eq!(
        command_for_key("t", false),
        Some(InputCommand::RandomizeRootMode)
    );
    assert_eq!(command_for_key("h", false), Some(InputCommand::ToggleHelp));
    assert_eq!(command_for_key("z", false), None);
    // Shift changes the detune magnitude.
    assert_eq!(
        command_for_key(".", true),
        Some(InputCommand::DetuneDelta(Cents(10.0)))
    );
    assert_eq!(
        command_for_key(".", false),
        Some(InputCommand::DetuneDelta(Cents(50.0)))
    );
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

#[test]
fn alternate_tunings_are_pairwise_distinct() {
    // Each N-TET pentatonic must yield distinct pitches so all three are audible and
    // every overlay::scale_name arm (which matches by slice value) stays reachable.
    let tunings = [
        core::TET19_PENTATONIC,
        core::TET24_PENTATONIC,
        core::TET31_PENTATONIC,
    ];
    for (i, a) in tunings.iter().enumerate() {
        for b in &tunings[i + 1..] {
            assert_ne!(a, b, "alternate tunings must not share pitches");
        }
    }
}
