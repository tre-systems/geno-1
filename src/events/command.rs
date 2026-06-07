//! Input commands: discrete user intents produced by the keyboard and pointer
//! handlers and drained by the frame loop. Funnelling all discrete input through
//! one queue keeps event closures from mutating engine/audio state directly.
//!
//! Continuous manipulation (voice drag, pointer swirl) stays frame-coupled and is
//! intentionally *not* represented here.

use crate::core::{Cents, Hz, MidiNote, VoiceIndex};
use crate::events::keymap::{mode_scale_for_digit, root_midi_for_key};

/// A discrete user intent, applied by the frame loop.
#[derive(Clone, Debug, PartialEq)]
pub enum InputCommand {
    SetRoot(MidiNote),
    SetScale(&'static [f32]),
    PresetPentatonic,
    ReseedAll,
    RandomizeRootMode,
    TogglePause,
    /// Change in BPM (applied with the keyboard's 40..240 clamp).
    TempoDelta(f32),
    /// Change in master gain (clamped 0..1).
    VolumeDelta(f32),
    ToggleMute,
    DetuneDelta(Cents),
    ResetDetune,
    ToggleFullscreen,
    ExitFullscreen,
    ToggleHelp,
    VoiceMute(VoiceIndex),
    VoiceSolo(VoiceIndex),
    VoiceReseed(VoiceIndex),
    PlayNote {
        voice: VoiceIndex,
        freq: Hz,
        velocity: f32,
        duration_sec: f64,
    },
    Ripple([f32; 2]),
}

/// Map a key (plus the shift modifier) to a discrete command. Pure and host-testable.
pub fn command_for_key(key: &str, shift: bool) -> Option<InputCommand> {
    if let Some(root) = root_midi_for_key(key) {
        return Some(InputCommand::SetRoot(root));
    }
    if let Some(scale) = mode_scale_for_digit(key) {
        return Some(InputCommand::SetScale(scale));
    }
    let cmd = match key {
        "p" | "P" => InputCommand::PresetPentatonic,
        "r" | "R" => InputCommand::ReseedAll,
        "t" | "T" => InputCommand::RandomizeRootMode,
        " " => InputCommand::TogglePause,
        "ArrowRight" | "+" | "=" => InputCommand::TempoDelta(5.0),
        "ArrowLeft" | "-" | "_" => InputCommand::TempoDelta(-5.0),
        "ArrowUp" => InputCommand::VolumeDelta(0.05),
        "ArrowDown" => InputCommand::VolumeDelta(-0.05),
        "m" | "M" => InputCommand::ToggleMute,
        "," => InputCommand::DetuneDelta(Cents(if shift { -10.0 } else { -50.0 })),
        "." => InputCommand::DetuneDelta(Cents(if shift { 10.0 } else { 50.0 })),
        "/" => InputCommand::ResetDetune,
        "Enter" => InputCommand::ToggleFullscreen,
        "Escape" => InputCommand::ExitFullscreen,
        "h" | "H" => InputCommand::ToggleHelp,
        _ => return None,
    };
    Some(cmd)
}

/// Whether a key's default browser action should be suppressed.
pub fn key_prevents_default(key: &str) -> bool {
    matches!(
        key,
        " " | "m" | "M" | "ArrowUp" | "ArrowDown" | "Enter" | "h" | "H"
    )
}
