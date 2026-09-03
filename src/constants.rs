/// Frame smoothing and interaction tuning constants.
///
/// These constants express intended behavior (e.g., time constants, clamp limits).
use glam::Vec3;

// Exponential decay rate for internal pulse energy
pub const PULSE_ENERGY_DECAY_PER_SEC: f32 = 1.6;

// Target smoothing time constants (seconds)
pub const PULSE_RISE_TAU_SEC: f32 = 0.10;
pub const PULSE_FALL_TAU_SEC: f32 = 0.45;

// Clamp on the per-frame delta so a stall (background tab, GC pause) eases the visuals
// back in instead of jerking them forward.
pub const MAX_FRAME_DT_SEC: f32 = 0.05;

// Pointer speed clamp (normalized units per second)
pub const POINTER_SPEED_MAX: f32 = 10.0;

// Inertial swirl spring parameters
pub const SWIRL_OMEGA: f32 = 1.1; // natural frequency
pub const SWIRL_DAMPING_RATIO: f32 = 0.5; // 0..1 critical at 1
pub const SWIRL_MAX_STEP_PER_SEC: f32 = 0.50; // cap motion per second (in uv units)

// Swirl energy blend weights
pub const SWIRL_TARGET_WEIGHT_POINTER: f32 = 0.2;
pub const SWIRL_TARGET_WEIGHT_VELOCITY: f32 = 0.35;
pub const SWIRL_TARGET_CLICK_BONUS: f32 = 0.5;
pub const SWIRL_ENERGY_BLEND_ALPHA: f32 = 0.15; // new = (1-α)*old + α*target

// Per-voice spatial sends mapping
pub const DIST_NORM_DIVISOR: f32 = 2.5;
pub const D_SEND_BASE: f32 = 0.08;
pub const D_SEND_SPAN: f32 = 0.38;
pub const R_SEND_BASE: f32 = 0.18;
pub const R_SEND_SPAN: f32 = 0.42;
pub const SEND_BOOST_COEFF: f32 = 0.45;
pub const D_SEND_CLAMP_MAX: f32 = 0.70;
pub const R_SEND_CLAMP_MAX: f32 = 0.90;

// Voice level mapping
pub const LEVEL_BASE: f32 = 0.32;
pub const LEVEL_SPAN: f32 = 0.24;

// Maximum number of concurrent in-flight notes (oscillator+gain pairs).
pub const MAX_POLYPHONY: usize = 24;

// Camera
// Z distance used by both picking and audio listener alignment.
pub const CAMERA_Z: f32 = 6.0;

// Voice interaction
pub const PICK_SPHERE_RADIUS: f32 = 0.5;
pub const SPREAD: Vec3 = glam::Vec3::new(3.0, 3.0, 3.0);
pub const Z_OFFSET: Vec3 = glam::Vec3::new(0.0, 0.0, -1.5);
pub const ENGINE_DRAG_MAX_RADIUS: f32 = 1.0;

// Post-processing defaults
pub const BLOOM_STRENGTH: f32 = 1.05;
pub const BLOOM_THRESHOLD: f32 = 0.54;

// Click-to-note mapping (pointer tap on empty space)
// Pitch spans CLICK_NOTE_MIDI_SPAN semitones across the canvas width from a base.
pub const CLICK_NOTE_BASE_MIDI: f32 = 60.0;
pub const CLICK_NOTE_MIDI_SPAN: f32 = 24.0;
pub const CLICK_VEL_BASE: f32 = 0.16;
pub const CLICK_VEL_SPAN: f32 = 0.32;
pub const CLICK_DUR_BASE_SEC: f64 = 0.35;
pub const CLICK_DUR_SPAN_SEC: f64 = 0.25;

// Pulse clamps: accumulated per-voice energy vs. smoothed visual pulse
pub const PULSE_ENERGY_MAX: f32 = 1.8;
pub const PULSE_MAX: f32 = 1.5;

// Analyser-driven ambient response
pub const ANALYSER_BINS_SAMPLED: usize = 16; // low bins averaged for ambient energy
pub const ANALYSER_DB_FLOOR: f32 = 100.0; // dB floor for normalizing analyser output
pub const AMBIENT_PULSE_GAIN: f32 = 0.05; // how much ambient energy lifts voice pulses
pub const AMBIENT_CLEAR_SCALE: f32 = 0.9; // ambient energy -> background clear amount

/// Runtime-tunable "feel" parameters for the interactive swirl. Seeded from the
/// constants above via `Default`; held by the frame loop so presets or live tuning
/// can vary it without a rebuild.
#[derive(Clone, Debug)]
pub struct Config {
    // Inertial swirl physics.
    pub swirl_omega: f32,
    pub swirl_damping_ratio: f32,
    pub swirl_max_step_per_sec: f32,
    // Swirl energy response.
    pub pointer_speed_max: f32,
    pub swirl_target_weight_pointer: f32,
    pub swirl_target_weight_velocity: f32,
    pub swirl_target_click_bonus: f32,
    pub swirl_energy_blend_alpha: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            swirl_omega: SWIRL_OMEGA,
            swirl_damping_ratio: SWIRL_DAMPING_RATIO,
            swirl_max_step_per_sec: SWIRL_MAX_STEP_PER_SEC,
            pointer_speed_max: POINTER_SPEED_MAX,
            swirl_target_weight_pointer: SWIRL_TARGET_WEIGHT_POINTER,
            swirl_target_weight_velocity: SWIRL_TARGET_WEIGHT_VELOCITY,
            swirl_target_click_bonus: SWIRL_TARGET_CLICK_BONUS,
            swirl_energy_blend_alpha: SWIRL_ENERGY_BLEND_ALPHA,
        }
    }
}
