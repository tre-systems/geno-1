//! Strongly-typed domain units so MIDI notes, frequencies, cents, tempo, and
//! voice indices can't be confused with one another or with raw numbers.
//!
//! These are plain `Copy` wrappers with a public inner value for ergonomics, but
//! the domain rules (ranges, conversions) live here so they are applied uniformly.

/// A MIDI note number. Fractional values are allowed for microtonal pitches.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct MidiNote(pub f32);

/// A frequency in Hertz.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Hz(pub f32);

/// A detune offset in cents (100 cents = one semitone).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Cents(pub f32);

/// Tempo in beats per minute.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Bpm(pub f32);

/// Index of a voice within the engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceIndex(pub usize);

impl Cents {
    /// Maximum absolute detune (±2 semitones).
    pub const LIMIT: f32 = 200.0;

    /// Clamp to the valid ±200¢ range.
    pub fn clamped(self) -> Self {
        Self(self.0.clamp(-Self::LIMIT, Self::LIMIT))
    }
}

impl Bpm {
    pub const MIN: f32 = 1.0;
    pub const MAX: f32 = 400.0;

    /// Clamp to a musically valid tempo range.
    pub fn clamped(self) -> Self {
        Self(self.0.clamp(Self::MIN, Self::MAX))
    }

    /// Whether this tempo is finite and positive.
    pub fn is_valid(self) -> bool {
        self.0.is_finite() && self.0 > 0.0
    }

    /// Length of one eighth-note step, in seconds.
    pub fn eighth_step_seconds(self) -> f64 {
        (60.0 / self.0 as f64) / 2.0
    }
}

impl MidiNote {
    /// Convert to Hertz (A4 = 440 Hz). Monotonic; +12 semitones doubles frequency.
    pub fn to_hz(self) -> Hz {
        Hz(440.0 * 2.0_f32.powf((self.0 - 69.0) / 12.0))
    }

    /// Convert to Hertz with a detune offset applied (detune is clamped first).
    pub fn to_hz_detuned(self, detune: Cents) -> Hz {
        Self(self.0 + detune.clamped().0 / 100.0).to_hz()
    }

    /// This note shifted by a number of semitones (e.g. scale degree + octave).
    pub fn shifted(self, semitones: f32) -> Self {
        Self(self.0 + semitones)
    }
}
