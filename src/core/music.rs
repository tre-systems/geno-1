use super::units::{Bpm, Cents, Hz, MidiNote, VoiceIndex};
use glam::Vec3;
use rand::prelude::*;
use std::time::Duration;

/// Basic oscillator shape used by synths in the web front-end.
#[derive(Clone, Copy, Debug)]
pub enum Waveform {
    Sine,
    Saw,
    Triangle,
}

/// Static configuration for a voice used at engine construction time.
#[derive(Clone, Debug)]
pub struct VoiceConfig {
    pub waveform: Waveform,
    /// Initial engine-space position (XZ plane; Y is typically 0).
    pub base_position: Vec3,
    /// Chance (0.0-1.0) that this voice triggers on each grid step.
    pub trigger_probability: f32,
    /// Octave adjustment relative to the root note (-2 to +2).
    pub octave_offset: i32,
    /// Base note duration in seconds.
    pub base_duration: f32,
}

/// A scheduled musical event produced by the engine for playback.
#[derive(Clone, Debug)]
pub struct NoteEvent {
    /// Which voice this event belongs to (index into `voices`).
    pub voice: VoiceIndex,
    /// Target pitch in Hertz (already converted from MIDI).
    pub freq: Hz,
    /// Normalized loudness 0..1 (mapped to the gain envelope).
    pub velocity: f32,
    /// Nominal duration in seconds (envelope length).
    pub duration_sec: f32,
}

/// Mutable runtime state per voice.
#[derive(Clone, Debug)]
pub struct VoiceState {
    pub position: Vec3,
    pub muted: bool,
}

/// Global engine parameters controlling tempo and scale.
///
/// - `bpm` controls the tempo of the scheduler (beats per minute)
/// - `scale` is the allowed pitch degree set, expressed as semitone offsets
/// - `root` is the MIDI note of the tonal center (e.g., 60 for C4)
/// - `detune` is the global detune offset in cents (-200 to +200)
#[derive(Clone, Debug)]
pub struct EngineParams {
    pub bpm: Bpm,
    pub scale: &'static [f32],
    pub root: MidiNote,
    pub detune: Cents,
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            bpm: Bpm(110.0),
            scale: C_MAJOR_PENTATONIC,
            root: MidiNote(60.0), // Middle C
            detune: Cents(0.0),
        }
    }
}

/// Default five-note scale centered around middle C.
pub const C_MAJOR_PENTATONIC: &[f32] = &[0.0, 2.0, 4.0, 7.0, 9.0, 12.0];

/// Diatonic modes (relative semitone degrees)
pub const IONIAN: &[f32] = &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0, 12.0]; // major
pub const DORIAN: &[f32] = &[0.0, 2.0, 3.0, 5.0, 7.0, 9.0, 10.0, 12.0];
pub const PHRYGIAN: &[f32] = &[0.0, 1.0, 3.0, 5.0, 7.0, 8.0, 10.0, 12.0];
pub const LYDIAN: &[f32] = &[0.0, 2.0, 4.0, 6.0, 7.0, 9.0, 11.0, 12.0];
pub const MIXOLYDIAN: &[f32] = &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 10.0, 12.0];
pub const AEOLIAN: &[f32] = &[0.0, 2.0, 3.0, 5.0, 7.0, 8.0, 10.0, 12.0]; // natural minor
pub const LOCRIAN: &[f32] = &[0.0, 1.0, 3.0, 5.0, 6.0, 8.0, 10.0, 12.0];

/// Equal pentatonics snapped to alternative equal-temperament grids. The octave (12
/// semitones) splits into N equal steps; each scale stacks the step closest to a fifth
/// of an octave (`round(N/5)` steps) four times, then closes on the octave. Degrees are
/// exact multiples of `12/N`, so the three tunings are audibly distinct.
pub const TET19_PENTATONIC: &[f32] = &[
    0.0,
    4.0 * 12.0 / 19.0,
    8.0 * 12.0 / 19.0,
    12.0 * 12.0 / 19.0,
    16.0 * 12.0 / 19.0,
    12.0,
];
pub const TET24_PENTATONIC: &[f32] = &[
    0.0,
    5.0 * 12.0 / 24.0,
    10.0 * 12.0 / 24.0,
    15.0 * 12.0 / 24.0,
    20.0 * 12.0 / 24.0,
    12.0,
];
pub const TET31_PENTATONIC: &[f32] = &[
    0.0,
    6.0 * 12.0 / 31.0,
    12.0 * 12.0 / 31.0,
    18.0 * 12.0 / 31.0,
    24.0 * 12.0 / 31.0,
    12.0,
];

/// Root notes in musical order (C D E F G A B), used by random selection.
pub const ROOTS_MUSICAL_ORDER: [MidiNote; 7] = [
    MidiNote(60.0),
    MidiNote(62.0),
    MidiNote(64.0),
    MidiNote(65.0),
    MidiNote(67.0),
    MidiNote(69.0),
    MidiNote(71.0),
];

/// Diatonic modes in order, used by random selection.
pub const MODES_ORDER: [&[f32]; 7] = [
    IONIAN, DORIAN, PHRYGIAN, LYDIAN, MIXOLYDIAN, AEOLIAN, LOCRIAN,
];

/// Random generative scheduler producing `NoteEvent`s on an eighth-note grid.
///
/// The engine maintains per-voice state and RNGs. On each tick, it advances an
/// internal accumulator based on the configured tempo (`params.bpm`) and emits
/// events aligned to an eighth-note grid. Voices have distinct trigger
/// probabilities, octave ranges, and base durations to create a simple texture.
///
/// Typical usage:
/// - Construct with `MusicEngine::new(configs, params, seed)`
/// - Call `tick(dt, now_sec, &mut out_events)` regularly to schedule audio
/// - Use `toggle_mute`, `toggle_solo`, `reseed_voice`, and `set_voice_position`
///   to interact with the engine state
pub struct MusicEngine {
    pub voices: Vec<VoiceState>,
    pub configs: Vec<VoiceConfig>,
    pub params: EngineParams,
    rngs: Vec<StdRng>,
    aux_rng: StdRng,
    solo_index: Option<usize>,
    beat_accum: f64,
}

impl MusicEngine {
    /// Construct a new engine with voices derived from the provided configs.
    pub fn new(configs: Vec<VoiceConfig>, params: EngineParams, seed: u64) -> Self {
        let voices = configs
            .iter()
            .map(|c| VoiceState {
                position: c.base_position,
                muted: false,
            })
            .collect::<Vec<_>>();

        // Derive per-voice RNGs from base seed so we can reseed voices independently
        let rngs = (0..voices.len())
            .map(|i| {
                let mix = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                StdRng::seed_from_u64(mix)
            })
            .collect::<Vec<_>>();
        // Separate RNG for non-voice randomization (root/mode), kept deterministic per seed.
        let aux_rng = StdRng::seed_from_u64(seed ^ 0xA5A5_5A5A_DEAD_BEEF);

        Self {
            voices,
            configs,
            params,
            rngs,
            aux_rng,
            solo_index: None,
            beat_accum: 0.0,
        }
    }

    /// Set beats-per-minute for the internal scheduler.
    pub fn set_bpm(&mut self, bpm: Bpm) {
        if !bpm.0.is_finite() {
            return;
        }
        self.params.bpm = bpm.clamped();
    }

    /// Set the global detune offset in cents.
    /// Range: -200 to +200 cents (±2 semitones)
    pub fn set_detune_cents(&mut self, detune: Cents) {
        self.params.detune = detune.clamped();
    }

    /// Adjust the global detune offset by the specified amount in cents.
    /// The result is clamped to the valid range of -200 to +200 cents.
    pub fn adjust_detune_cents(&mut self, delta: Cents) {
        self.set_detune_cents(Cents(self.params.detune.0 + delta.0));
    }

    /// Reset the global detune offset to 0 cents (no detune).
    pub fn reset_detune(&mut self) {
        self.params.detune = Cents(0.0);
    }

    /// Toggle mute flag for a voice.
    pub fn toggle_mute(&mut self, voice: VoiceIndex) {
        if let Some(v) = self.voices.get_mut(voice.0) {
            v.muted = !v.muted;
        }
    }

    /// Update the engine-space position of a voice.
    pub fn set_voice_position(&mut self, voice: VoiceIndex, pos: Vec3) {
        if let Some(v) = self.voices.get_mut(voice.0) {
            v.position = pos;
        }
    }

    /// Reseed the per-voice RNG. If `seed` is None, a new random seed is chosen.
    pub fn reseed_voice(&mut self, voice: VoiceIndex, seed: Option<u64>) {
        if let Some(r) = self.rngs.get_mut(voice.0) {
            let new_seed = seed.unwrap_or_else(|| r.gen());
            *r = StdRng::seed_from_u64(new_seed);
        }
    }

    /// Randomly choose a new root note and diatonic mode using the engine's
    /// seeded auxiliary RNG (deterministic for a given seed, host-testable).
    pub fn randomize_root_and_mode(&mut self) {
        let ri = self.aux_rng.gen_range(0..ROOTS_MUSICAL_ORDER.len());
        let mi = self.aux_rng.gen_range(0..MODES_ORDER.len());
        self.params.root = ROOTS_MUSICAL_ORDER[ri];
        self.params.scale = MODES_ORDER[mi];
    }

    /// Solo a voice. Toggling solo on the same voice clears solo mode.
    pub fn toggle_solo(&mut self, voice: VoiceIndex) {
        match self.solo_index {
            Some(idx) if idx == voice.0 => {
                // Clear solo -> unmute all
                self.solo_index = None;
                for v in &mut self.voices {
                    v.muted = false;
                }
            }
            _ => {
                self.solo_index = Some(voice.0);
                for (i, v) in self.voices.iter_mut().enumerate() {
                    v.muted = i != voice.0;
                }
            }
        }
    }

    /// Advance the scheduler by `dt`, pushing any newly scheduled `NoteEvent`s into `out_events`.
    pub fn tick(&mut self, dt: Duration, out_events: &mut Vec<NoteEvent>) {
        let bpm = self.params.bpm;
        if !bpm.is_valid() {
            return;
        }
        let step = bpm.eighth_step_seconds();
        if !step.is_finite() || step <= 0.0 {
            return;
        }
        self.beat_accum += dt.as_secs_f64();
        while self.beat_accum >= step {
            self.beat_accum -= step;
            self.step(out_events);
        }
    }

    /// Advance one eighth-note grid step, pushing any triggered notes. This is the
    /// per-step entry the audio scheduler drives directly on the audio clock; `tick`
    /// calls it via its wall-clock accumulator (used by host tests).
    pub fn step(&mut self, out_events: &mut Vec<NoteEvent>) {
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.muted {
                continue;
            }
            let prob = self.configs[i].trigger_probability;
            let rng = &mut self.rngs[i];
            if rng.gen::<f32>() < prob {
                let degree = *self.params.scale.choose(rng).unwrap_or(&0.0);
                let octave = self.configs[i].octave_offset;
                let note = self.params.root.shifted(degree + (octave * 12) as f32);
                let freq = note.to_hz_detuned(self.params.detune);
                let vel = 0.4 + rng.gen::<f32>() * 0.6;
                let dur = self.configs[i].base_duration + rng.gen::<f32>() * 0.2;
                out_events.push(NoteEvent {
                    voice: VoiceIndex(i),
                    freq,
                    velocity: vel,
                    duration_sec: dur,
                });
            }
        }
    }
}

/// Convert a MIDI note number to Hertz (A4=440 Hz).
///
/// Monotonic and exhibits octave symmetry: +12 semitones doubles the frequency.
/// Supports fractional MIDI values for microtonal precision.
pub fn midi_to_hz(midi: f32) -> f32 {
    MidiNote(midi).to_hz().0
}

/// Convert a MIDI note number to Hertz with detune offset in cents.
///
/// The detune_cents parameter allows for microtonal adjustments:
/// - Positive values raise the pitch (e.g., +50¢ = quarter tone sharp)
/// - Negative values lower the pitch (e.g., -50¢ = quarter tone flat)
/// - Range: -200 to +200 cents (±2 semitones)
pub fn midi_to_hz_with_detune(midi: f32, detune_cents: f32) -> f32 {
    MidiNote(midi).to_hz_detuned(Cents(detune_cents)).0
}

/// Map the current harmony (root note + scale) to a scene-colour hint for the visuals.
///
/// Returns `(hue_shift, warmth)`, both in `0.0..=1.0`: `hue_shift` rotates with the root's
/// pitch class (so changing key recolours the scene), and `warmth` rises with the scale's
/// brightness — major-third modes read warm, minor-third modes cool. Pure, so the mapping
/// is deterministic and host-testable.
pub fn harmony_color(root: MidiNote, scale: &[f32]) -> (f32, f32) {
    let pitch_class = (root.0.round() as i32).rem_euclid(12) as f32;
    let hue_shift = pitch_class / 12.0;
    // The third (scale[2] for these scales) is the dominant major/minor brightness cue; the
    // mean degree adds finer modal ordering (e.g. Lydian brighter than Ionian).
    let third = scale.get(2).copied().unwrap_or(4.0);
    let mean = if scale.is_empty() {
        6.0
    } else {
        scale.iter().sum::<f32>() / scale.len() as f32
    };
    let warmth = (0.5 + (third - 3.5) * 0.35 + (mean - 6.0) * 0.15).clamp(0.0, 1.0);
    (hue_shift, warmth)
}

/// Normalise a frequency to `0.0..=1.0` over the instrument's working range (about MIDI
/// 48..84), for mapping a note's pitch to visual brightness. Pure and host-testable.
pub fn pitch_norm(hz: f32) -> f32 {
    if hz <= 0.0 {
        return 0.0;
    }
    let midi = 69.0 + 12.0 * (hz / 440.0).log2();
    ((midi - 48.0) / 36.0).clamp(0.0, 1.0)
}
