use crate::core::{
    MidiNote, AEOLIAN, DORIAN, IONIAN, LOCRIAN, LYDIAN, MIXOLYDIAN, PHRYGIAN, TET19_PENTATONIC,
    TET24_PENTATONIC, TET31_PENTATONIC,
};

#[inline]
pub fn root_midi_for_key(key: &str) -> Option<MidiNote> {
    match key {
        "a" | "A" => Some(MidiNote(69.0)), // A4
        "b" | "B" => Some(MidiNote(71.0)), // B4
        "c" | "C" => Some(MidiNote(60.0)), // C4 (middle C)
        "d" | "D" => Some(MidiNote(62.0)), // D4
        "e" | "E" => Some(MidiNote(64.0)), // E4
        "f" | "F" => Some(MidiNote(65.0)), // F4
        "g" | "G" => Some(MidiNote(67.0)), // G4
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
