use super::music::{MusicEngine, DRIFT_DORIAN_JUST, DRIFT_JUST_PENTATONIC};
use super::units::{Bpm, Cents, MidiNote};
use glam::Vec3;
use std::f32::consts::TAU;

pub const PIECE_TITLE: &str = "Geno-1: Drift Lattice";
pub const PIECE_SUBTITLE: &str = "a generative sound sculpture for three spatial voices";
pub const PIECE_CREDIT: &str = "Multivibrator";
pub const EXPORT_TAIL_SEC: f64 = 6.0;

#[derive(Clone, Copy, Debug)]
pub struct SculptureAudio {
    pub pad_freqs: [f32; 3],
    pub pad_gain: f32,
    pub pad_cutoff_hz: f32,
    pub pad_detune_cents: f32,
    pub analog_drift_cents: f32,
    pub sub_freq: f32,
    pub sub_gain: f32,
    pub star_gain: f32,
    pub star_cutoff_hz: f32,
    pub shimmer_gain: f32,
    pub noise_gain: f32,
    pub field_pan: f32,
    pub reverb_wet: f32,
    pub delay_wet: f32,
    pub delay_feedback: f32,
    pub analog_drive: f32,
    pub analog_wet: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PieceMoment {
    pub bpm: Bpm,
    pub root: MidiNote,
    pub scale: &'static [f32],
    pub detune: Cents,
    pub voice_positions: [Vec3; 3],
    pub trigger_probabilities: [f32; 3],
    pub base_durations: [f32; 3],
    pub swirl_uv: [f32; 2],
    pub swirl_energy: f32,
    pub ripple_uv: [f32; 2],
    pub ripple_index: u32,
    pub audio: SculptureAudio,
}

#[derive(Clone, Copy, Debug)]
pub struct PieceArrangement {
    pub duration: f64,
    pub seed: u32,
}

impl PieceArrangement {
    pub fn new(duration: f64, seed: u32) -> Self {
        Self {
            duration: duration.max(1.0),
            seed,
        }
    }

    fn progress(&self, t: f64) -> f32 {
        (t / self.duration).clamp(0.0, 1.0) as f32
    }

    pub fn moment(&self, t: f64) -> PieceMoment {
        let u = self.progress(t);
        let seed_phase = TAU * hash01(self.seed, 3);
        let slow = (TAU * (1.7 * u) + seed_phase).sin();
        let slow_b = (TAU * (2.3 * u) + TAU * hash01(self.seed, 5)).sin();
        let form = composed_form(u);
        let a = (form.energy + 0.035 * slow.abs()).clamp(0.0, 1.0);
        let root = match form.section {
            0 => MidiNote(45.0), // A2: settling ground
            1 => MidiNote(50.0), // D3: first opening
            2 => MidiNote(43.0), // G2: wider body
            3 => MidiNote(52.0), // E3: turning point
            _ => MidiNote(45.0), // A2: resolution
        };
        let scale = if matches!(form.section, 0 | 3 | 4) {
            DRIFT_DORIAN_JUST
        } else {
            DRIFT_JUST_PENTATONIC
        };
        let bpm = Bpm(54.0 + 13.0 * form.density + 1.8 * slow).clamped();
        let detune = Cents((-4.0 + 8.0 * slow_b + 4.0 * (a - 0.5)).clamp(-24.0, 24.0));
        let orbit = TAU * (0.92 * u) + seed_phase + 0.16 * form.local;
        let radius = 0.30 + 0.42 * a;
        let voice_positions = std::array::from_fn(|i| {
            let fi = i as f32;
            let wobble = 0.045 * (TAU * (u * (1.2 + fi * 0.19)) + fi).sin();
            let angle = orbit + fi * TAU / 3.0 + 0.25 * slow_b;
            Vec3::new(
                (radius + wobble) * angle.cos(),
                0.0,
                (radius - wobble) * angle.sin(),
            )
        });
        let trigger_probabilities = [
            (0.07 + 0.15 * form.density + 0.025 * slow.max(0.0)).clamp(0.03, 0.30),
            (0.045 + 0.12 * form.density + 0.020 * slow_b.max(0.0)).clamp(0.02, 0.24),
            (0.025 + 0.08 * form.density + 0.018 * (1.0 - slow.abs())).clamp(0.01, 0.18),
        ];
        let base_durations = [
            2.20 + 1.25 * (1.0 - a) + 0.35 * slow.abs(),
            1.45 + 0.95 * (1.0 - a) + 0.20 * slow_b.abs(),
            3.10 + 1.25 * (1.0 - a),
        ];
        let swirl_uv = [
            (0.5 + 0.26 * (TAU * (0.82 * u) + seed_phase).sin()).clamp(0.12, 0.88),
            (0.5 + 0.23 * (TAU * (1.08 * u) + TAU * hash01(self.seed, 7)).cos()).clamp(0.14, 0.86),
        ];
        let swirl_energy = (0.18 + 0.50 * a + 0.08 * slow.abs()).clamp(0.0, 0.82);
        let ripple_period = (10.5 - 3.0 * form.density + 2.0 * hash01(self.seed, 17)) as f64;
        let ripple_index = (t / ripple_period).floor().max(0.0) as u32;
        let ripple_phase = ripple_index as f32 * 1.618 + seed_phase;
        let ripple_uv = [
            (0.5 + 0.34 * ripple_phase.sin()).clamp(0.12, 0.88),
            (0.5 + 0.30 * (ripple_phase * 1.31).cos()).clamp(0.14, 0.86),
        ];
        let audio = sculpture_audio(AudioShape {
            root,
            detune,
            arc: a,
            slow,
            slow_b,
            swirl_energy,
            swirl_uv,
            progress: u,
            density: form.density,
            seed_bias: hash01(self.seed, 29),
        });

        PieceMoment {
            bpm,
            root,
            scale,
            detune,
            voice_positions,
            trigger_probabilities,
            base_durations,
            swirl_uv,
            swirl_energy,
            ripple_uv,
            ripple_index,
            audio,
        }
    }

    pub fn timeline(&self, dt: f64) -> Vec<(f64, PieceMoment)> {
        let dt = dt.max(1.0 / 120.0);
        let mut out = Vec::with_capacity((self.duration / dt) as usize + 2);
        let mut t = 0.0;
        while t < self.duration {
            out.push((t, self.moment(t)));
            t += dt;
        }
        out.push((self.duration, self.moment(self.duration)));
        out
    }
}

impl PieceMoment {
    pub fn apply_to_engine(&self, engine: &mut MusicEngine) {
        engine.params.bpm = self.bpm;
        engine.params.root = self.root;
        engine.params.scale = self.scale;
        engine.params.detune = self.detune;
        for i in 0..engine.voices.len().min(3) {
            engine.voices[i].position = self.voice_positions[i];
            engine.configs[i].trigger_probability = self.trigger_probabilities[i];
            engine.configs[i].base_duration = self.base_durations[i];
        }
    }
}

pub fn live_sculpture_audio(
    root: MidiNote,
    detune: Cents,
    swirl_energy: f32,
    swirl_uv: [f32; 2],
    t: f32,
) -> SculptureAudio {
    let slow = (0.13 * t).sin();
    let slow_b = (0.071 * t + 1.6).sin();
    sculpture_audio(AudioShape {
        root,
        detune,
        arc: (0.28 + 0.55 * swirl_energy).clamp(0.0, 1.0),
        slow,
        slow_b,
        swirl_energy,
        swirl_uv,
        progress: (t * 0.01).fract(),
        density: swirl_energy,
        seed_bias: 0.5,
    })
}

#[derive(Clone, Copy, Debug)]
struct FormState {
    section: u8,
    local: f32,
    energy: f32,
    density: f32,
}

fn composed_form(u: f32) -> FormState {
    let (section, start, end, e0, e1, d0, d1) = if u < 0.18 {
        (0, 0.0, 0.18, 0.12, 0.34, 0.10, 0.24)
    } else if u < 0.42 {
        (1, 0.18, 0.42, 0.34, 0.56, 0.24, 0.44)
    } else if u < 0.66 {
        (2, 0.42, 0.66, 0.56, 0.72, 0.42, 0.62)
    } else if u < 0.84 {
        (3, 0.66, 0.84, 0.62, 0.48, 0.46, 0.34)
    } else {
        (4, 0.84, 1.0, 0.48, 0.16, 0.30, 0.08)
    };
    let local = ((u - start) / (end - start)).clamp(0.0, 1.0);
    let shaped = smoother(local);
    let section_breath = (TAU * local).sin().max(0.0);
    FormState {
        section,
        local,
        energy: (lerp(e0, e1, shaped) + 0.045 * section_breath).clamp(0.0, 1.0),
        density: (lerp(d0, d1, shaped) + 0.035 * section_breath).clamp(0.0, 1.0),
    }
}

struct AudioShape {
    root: MidiNote,
    detune: Cents,
    arc: f32,
    slow: f32,
    slow_b: f32,
    swirl_energy: f32,
    swirl_uv: [f32; 2],
    progress: f32,
    density: f32,
    seed_bias: f32,
}

fn sculpture_audio(input: AudioShape) -> SculptureAudio {
    let AudioShape {
        root,
        detune,
        arc,
        slow,
        slow_b,
        swirl_energy,
        swirl_uv,
        progress,
        density,
        seed_bias,
    } = input;
    let echo = (swirl_uv[0] - swirl_uv[1]).abs();
    let fizz = ((swirl_uv[0] + swirl_uv[1]) * 0.5).clamp(0.0, 1.0);
    let analog_phase = TAU * (progress * (1.4 + 0.45 * seed_bias) + seed_bias);
    let analog_wander = ((analog_phase.sin()
        + 0.55 * (analog_phase * 1.618 + slow_b).sin()
        + 0.35 * (analog_phase * 2.414 + slow).sin())
        / 1.90)
        .clamp(-1.0, 1.0);
    let bend = 0.55 * slow + 0.55 * (arc - 0.5) + detune.0 / 180.0;
    let intervals = [0.0, 7.02, 12.0 + 0.24 * slow_b];
    let mut pad_freqs = [0.0; 3];
    for (f, interval) in pad_freqs.iter_mut().zip(intervals) {
        *f = midi_to_hz(root.0 - 24.0 + interval + bend);
    }
    let cutoff_oct = -0.05
        + 0.72 * arc
        + 0.14 * swirl_energy
        + 0.08 * slow_b.max(0.0)
        + 0.04 * analog_wander
        + 0.03 * (seed_bias - 0.5);
    let pad_cutoff_hz = (220.0 * 2.0_f32.powf(cutoff_oct)).clamp(130.0, 1150.0);
    let star_cutoff_oct = 0.24 * arc + 0.08 * swirl_energy + 0.03 * analog_wander;
    SculptureAudio {
        pad_freqs,
        pad_gain: (0.055 + 0.125 * arc + 0.012 * slow.abs()).clamp(0.0, 0.22),
        pad_cutoff_hz,
        pad_detune_cents: (2.0 + 5.5 * arc + 3.8 * swirl_energy + 1.4 * slow_b.abs())
            .clamp(0.0, 14.0),
        analog_drift_cents: (1.6
            + 1.8 * arc
            + 1.2 * swirl_energy
            + 0.6 * seed_bias
            + 0.55 * analog_wander.abs())
        .clamp(0.8, 5.4),
        sub_freq: (pad_freqs[0] * 0.5).clamp(34.0, 90.0),
        sub_gain: (0.038 + 0.065 * arc).clamp(0.0, 0.13),
        star_gain: (0.0015 + 0.012 * density + 0.005 * swirl_energy).clamp(0.0, 0.025),
        star_cutoff_hz: (540.0 * 2.0_f32.powf(star_cutoff_oct)).clamp(300.0, 900.0),
        shimmer_gain: (0.0015 + 0.010 * density + 0.006 * swirl_energy).clamp(0.0, 0.020),
        noise_gain: (0.075 + 0.105 * (1.0 - arc) + 0.055 * (1.0 - swirl_uv[1]).max(0.0))
            .clamp(0.0, 0.23),
        field_pan: ((swirl_uv[0] - 0.5) * 1.55 + 0.14 * slow).clamp(-0.88, 0.88),
        reverb_wet: (0.42 + 0.20 * swirl_energy + 0.24 * (1.0 - arc) + 0.04 * echo)
            .clamp(0.0, 0.86),
        delay_wet: (0.15
            + 0.14 * swirl_energy
            + 0.09 * echo
            + 0.025 * slow_b.max(0.0)
            + 0.020 * analog_wander.max(0.0))
        .clamp(0.0, 0.48),
        delay_feedback: (0.28 + 0.09 * arc + 0.07 * swirl_energy + 0.03 * echo).clamp(0.0, 0.52),
        analog_drive: (0.68
            + 0.13 * swirl_energy
            + 0.08 * arc
            + 0.05 * fizz
            + 0.05 * (seed_bias - 0.5)
            + 0.03 * analog_wander)
            .clamp(0.50, 1.05),
        analog_wet: (0.12
            + 0.045 * arc
            + 0.035 * swirl_energy
            + 0.020 * fizz
            + 0.015 * analog_wander)
            .clamp(0.06, 0.24),
    }
}

fn midi_to_hz(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}

fn smoother(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hash01(seed: u32, salt: u32) -> f32 {
    let mut h = seed.wrapping_add(salt.wrapping_mul(0x9E37_79B9));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h % 100_000) as f32 / 100_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrangement_builds_and_resolves() {
        let arr = PieceArrangement::new(360.0, 7);
        let intro = arr.moment(18.0);
        let peak = arr.moment(220.0);
        let outro = arr.moment(352.0);

        assert!(peak.swirl_energy > intro.swirl_energy);
        assert!(peak.audio.pad_gain > intro.audio.pad_gain);
        assert!(peak.audio.pad_gain > outro.audio.pad_gain);
        assert!(peak.audio.star_gain > outro.audio.star_gain);
    }

    #[test]
    fn timeline_is_monotonic_and_bounded() {
        let arr = PieceArrangement::new(120.0, 3);
        let timeline = arr.timeline(1.0 / 30.0);
        let mut last_t = -1.0;
        for (t, m) in timeline {
            assert!(t >= last_t);
            last_t = t;
            assert!((0.0..=1.0).contains(&m.swirl_energy));
            assert!((0.0..=1.0).contains(&m.swirl_uv[0]));
            assert!((0.0..=1.0).contains(&m.swirl_uv[1]));
            assert!((-0.88..=0.88).contains(&m.audio.field_pan));
            assert!(m.audio.pad_freqs.iter().all(|f| f.is_finite() && *f > 0.0));
            assert!((0.8..=5.4).contains(&m.audio.analog_drift_cents));
            assert!((0.50..=1.05).contains(&m.audio.analog_drive));
            assert!((0.06..=0.24).contains(&m.audio.analog_wet));
        }
    }
}
