use crate::core::{
    AEOLIAN, DORIAN, IONIAN, LOCRIAN, LYDIAN, MIXOLYDIAN, PHRYGIAN, TET19_PENTATONIC,
    TET24_PENTATONIC, TET31_PENTATONIC,
};

#[inline]
pub fn root_midi_for_key(key: &str) -> Option<i32> {
    match key {
        "a" | "A" => Some(69), // A4
        "b" | "B" => Some(71), // B4
        "c" | "C" => Some(60), // C4 (middle C)
        "d" | "D" => Some(62), // D4
        "e" | "E" => Some(64), // E4
        "f" | "F" => Some(65), // F4
        "g" | "G" => Some(67), // G4
        _ => None,
    }
}

#[inline]
pub fn mode_scale_for_digit(key: &str) -> Option<&'static [f32]> {
    match key {
        "1" => Some(IONIAN),
        "2" => Some(DORIAN),
        "3" => Some(PHRYGIAN),
        "4" => Some(LYDIAN),
        "5" => Some(MIXOLYDIAN),
        "6" => Some(AEOLIAN),
        "7" => Some(LOCRIAN),
        "8" => Some(TET19_PENTATONIC),
        "9" => Some(TET24_PENTATONIC),
        "0" => Some(TET31_PENTATONIC),
        _ => None,
    }
}
