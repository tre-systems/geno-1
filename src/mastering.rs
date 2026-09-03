//! Offline mastering and analysis for the composed-piece audio export.
//!
//! Pure Rust DSP on planar stereo `f32` buffers — no web/audio dependency — so it
//! unit-tests natively like `music.rs` and `scenarios.rs`. It runs the polish a
//! mastering engineer would apply to a finished mixdown, automatically, so a
//! visitor can export a release-ready file with no audio expertise:
//!
//! 1. subsonic high-pass (clears the rumble that just wastes headroom),
//! 2. mono-sum the deep bass (so it translates on phones and never phase-cancels),
//! 3. measure ITU-R BS.1770 integrated loudness,
//! 4. apply broadband gain to the target LUFS (streaming-normalisation aware),
//! 5. a look-ahead, stereo-linked limiter to the true-peak ceiling, and
//! 6. encode 24-bit WAV.
//!
//! A [`MasterReport`] captures loudness in/out, true peak, stereo correlation, and
//! spectral tilt, so the export can tell the user whether the result will sound
//! good for most listeners.

use std::f32::consts::PI;

/// Mastering parameters. Defaults are tuned for streaming ambient: quieter than the
/// ~-14 LUFS platform target so the genre's dynamics survive normalisation, with a
/// -1 dBTP ceiling so it stays clean through lossy codecs (AAC/Ogg).
#[derive(Clone, Copy, Debug)]
pub struct MasterSettings {
    pub sample_rate: u32,
    pub target_lufs: f32,
    pub true_peak_ceiling_db: f32,
}

impl Default for MasterSettings {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            target_lufs: -16.0,
            true_peak_ceiling_db: -1.0,
        }
    }
}

/// What the master did and how the result measures — surfaced to the user so they
/// can trust the export (or tweak the target).
#[derive(Clone, Copy, Debug, Default)]
pub struct MasterReport {
    pub duration_secs: f32,
    /// Integrated loudness of the mix before and after mastering (LUFS).
    pub lufs_in: f32,
    pub lufs_out: f32,
    /// Final true peak (dBTP, 4× oversampled) and sample peak (dBFS).
    pub true_peak_db: f32,
    pub sample_peak_db: f32,
    /// Broadband gain applied to reach the target (dB).
    pub gain_db: f32,
    /// Whether the peak limiter engaged.
    pub limited: bool,
    /// Stereo correlation, -1 (out of phase) .. 1 (mono). Healthy mixes sit > ~0.3.
    pub stereo_correlation: f32,
    /// Spectral tilt: high-band vs low-band energy (dB). Negative = darker (more
    /// lows), the usual ambient balance.
    pub spectral_tilt_db: f32,
}

/// A Direct-Form-I biquad (coefficients already normalised by `a0`).
#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    /// Filter a buffer in place. State is local, so each call is independent.
    fn process(&self, buf: &mut [f32]) {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0_f32, 0.0, 0.0, 0.0);
        for s in buf.iter_mut() {
            let x0 = *s;
            let y0 = self.b0 * x0 + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            x2 = x1;
            x1 = x0;
            y2 = y1;
            y1 = y0;
            *s = y0;
        }
    }

    /// Mean square of the signal after filtering a copy — used by the loudness and
    /// tilt measurements without mutating the input.
    fn mean_square(&self, buf: &[f32]) -> f64 {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0_f32, 0.0, 0.0, 0.0);
        let mut acc = 0.0_f64;
        for &x0 in buf {
            let y0 = self.b0 * x0 + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            x2 = x1;
            x1 = x0;
            y2 = y1;
            y1 = y0;
            acc += (y0 as f64) * (y0 as f64);
        }
        acc / buf.len().max(1) as f64
    }
}

/// RBJ-cookbook low-pass at `fc` Hz, quality `q`.
fn rbj_lowpass(sr: f32, fc: f32, q: f32) -> Biquad {
    let w0 = 2.0 * PI * (fc / sr);
    let (sw, cw) = (w0.sin(), w0.cos());
    let alpha = sw / (2.0 * q);
    let a0 = 1.0 + alpha;
    Biquad {
        b0: (1.0 - cw) / 2.0 / a0,
        b1: (1.0 - cw) / a0,
        b2: (1.0 - cw) / 2.0 / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
    }
}

/// RBJ-cookbook high-pass at `fc` Hz, quality `q`.
fn rbj_highpass(sr: f32, fc: f32, q: f32) -> Biquad {
    let w0 = 2.0 * PI * (fc / sr);
    let (sw, cw) = (w0.sin(), w0.cos());
    let alpha = sw / (2.0 * q);
    let a0 = 1.0 + alpha;
    Biquad {
        b0: (1.0 + cw) / 2.0 / a0,
        b1: -(1.0 + cw) / a0,
        b2: (1.0 + cw) / 2.0 / a0,
        a1: -2.0 * cw / a0,
        a2: (1.0 - alpha) / a0,
    }
}

/// The ITU-R BS.1770 K-weighting pair at 48 kHz (a high-shelf "head" filter then an
/// RLB high-pass). The export always renders at 48 kHz so these canonical
/// coefficients are exact.
const K_SHELF: Biquad = Biquad {
    b0: 1.535_124_9,
    b1: -2.691_696_2,
    b2: 1.198_392_8,
    a1: -1.690_659_3,
    a2: 0.732_480_8,
};
const K_HPF: Biquad = Biquad {
    b0: 1.0,
    b1: -2.0,
    b2: 1.0,
    a1: -1.990_047_5,
    a2: 0.990_072_3,
};

/// ITU-R BS.1770 integrated loudness (LUFS) of a stereo signal, with the absolute
/// (-70 LUFS) and -10 LU relative gates. Returns -f32::INFINITY for silence.
fn integrated_lufs(left: &[f32], right: &[f32], sr: u32) -> f32 {
    // K-weight each channel (shelf → HPF).
    let kw = |ch: &[f32]| -> Vec<f32> {
        let mut v = ch.to_vec();
        K_SHELF.process(&mut v);
        K_HPF.process(&mut v);
        v
    };
    let (kl, kr) = (kw(left), kw(right));
    // 400 ms blocks, 75% overlap (100 ms hop).
    let block = (0.4 * sr as f32) as usize;
    let hop = (0.1 * sr as f32) as usize;
    if block == 0 || kl.len() < block {
        return f32::NEG_INFINITY;
    }
    let mut z = Vec::new(); // per-block mean-square sum across channels
    let mut i = 0;
    while i + block <= kl.len() {
        let ms = |ch: &[f32]| -> f64 {
            ch[i..i + block]
                .iter()
                .map(|&s| (s as f64) * (s as f64))
                .sum::<f64>()
                / block as f64
        };
        z.push(ms(&kl) + ms(&kr));
        i += hop;
    }
    let loud = |zj: f64| -0.691 + 10.0 * zj.max(1e-12).log10();
    // Absolute gate at -70 LUFS.
    let abs_gated: Vec<f64> = z.iter().copied().filter(|&zj| loud(zj) >= -70.0).collect();
    if abs_gated.is_empty() {
        return f32::NEG_INFINITY;
    }
    // Relative gate at -10 LU below the abs-gated mean loudness.
    let mean_abs = abs_gated.iter().sum::<f64>() / abs_gated.len() as f64;
    let rel_thresh = loud(mean_abs) - 10.0;
    let gated: Vec<f64> = z
        .iter()
        .copied()
        .filter(|&zj| loud(zj) >= rel_thresh)
        .collect();
    let used = if gated.is_empty() { abs_gated } else { gated };
    let mean = used.iter().sum::<f64>() / used.len() as f64;
    loud(mean) as f32
}

/// Polyphase 4× oversampler used only to measure inter-sample (true) peak: a
/// windowed-sinc low-pass split into 4 phases. Generated once per call.
struct TruePeak {
    phases: [[f32; TruePeak::TAPS]; 4],
}

impl TruePeak {
    const TAPS: usize = 12;

    fn new() -> Self {
        let len = 4 * Self::TAPS; // 48-tap prototype
        let center = (len - 1) as f32 / 2.0;
        let mut proto = vec![0.0_f32; len];
        for (k, c) in proto.iter_mut().enumerate() {
            let x = k as f32 - center;
            // sinc at cutoff = Nyquist of the base rate (0.25 of the 4× rate).
            let sinc = if x.abs() < 1e-6 {
                1.0
            } else {
                (PI * x / 4.0).sin() / (PI * x / 4.0)
            };
            // Hann window.
            let w = 0.5 - 0.5 * (2.0 * PI * k as f32 / (len - 1) as f32).cos();
            *c = sinc * w;
        }
        let mut phases = [[0.0_f32; Self::TAPS]; 4];
        for (p, phase) in phases.iter_mut().enumerate() {
            for (t, coef) in phase.iter_mut().enumerate() {
                // Unity per-phase gain so the peak isn't scaled by the upsampler.
                *coef = proto[p + 4 * t];
            }
        }
        Self { phases }
    }

    /// Peak magnitude of the 4×-oversampled signal (linear).
    fn peak(&self, ch: &[f32]) -> f32 {
        let n = ch.len();
        let mut peak = 0.0_f32;
        let half = Self::TAPS as isize / 2;
        for i in 0..n {
            for phase in &self.phases {
                let mut acc = 0.0_f32;
                for (t, &coef) in phase.iter().enumerate() {
                    let idx = i as isize + half - t as isize;
                    if idx >= 0 && (idx as usize) < n {
                        acc += coef * ch[idx as usize];
                    }
                }
                peak = peak.max(acc.abs());
            }
        }
        peak
    }
}

fn db_to_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn lin_to_db(lin: f32) -> f32 {
    20.0 * lin.max(1e-9).log10()
}

/// Sum the energy below `fc` to mono (keeping the highs stereo), so the deep bass
/// stays phase-coherent and translates on small/mono speakers.
fn mono_bass(left: &mut [f32], right: &mut [f32], sr: f32, fc: f32) {
    let lp = rbj_lowpass(sr, fc, 0.707);
    let mut low_l = left.to_vec();
    let mut low_r = right.to_vec();
    lp.process(&mut low_l);
    lp.process(&mut low_r);
    for i in 0..left.len() {
        let mono = 0.5 * (low_l[i] + low_r[i]);
        left[i] = (left[i] - low_l[i]) + mono;
        right[i] = (right[i] - low_r[i]) + mono;
    }
}

/// A stereo-linked look-ahead peak limiter that bounds the sample peak to `ceiling`.
/// Returns whether it engaged. Lookahead lets the gain duck *before* a transient
/// (via a rolling minimum of the required gain), and a slow release avoids pumping.
fn limit(left: &mut [f32], right: &mut [f32], sr: f32, ceiling: f32) -> bool {
    let n = left.len();
    if n == 0 {
        return false;
    }
    let la = ((0.005 * sr) as usize).max(1); // 5 ms lookahead
    let rel = (-1.0 / (0.120 * sr)).exp(); // ~120 ms release

    // Instantaneous gain needed to keep each sample under the ceiling.
    let mut need = vec![1.0_f32; n];
    for i in 0..n {
        let p = left[i].abs().max(right[i].abs());
        if p > ceiling {
            need[i] = ceiling / p;
        }
    }
    // Rolling minimum over the lookahead window [i, i+la], so the gain is already
    // down when a peak arrives. Monotonic deque (front = window minimum), O(n).
    let mut look = vec![1.0_f32; n];
    let mut dq: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut pushed = 0usize; // next index not yet added to the window
    #[allow(clippy::needless_range_loop)] // i is the window left edge, not just an index
    for i in 0..n {
        let right = (i + la).min(n - 1);
        while pushed <= right {
            while let Some(&b) = dq.back() {
                if need[b] >= need[pushed] {
                    dq.pop_back();
                } else {
                    break;
                }
            }
            dq.push_back(pushed);
            pushed += 1;
        }
        while let Some(&f) = dq.front() {
            if f < i {
                dq.pop_front();
            } else {
                break;
            }
        }
        look[i] = need[*dq.front().unwrap()];
    }
    // Forward gain: snap down instantly to the looked-ahead minimum, release slowly.
    let mut engaged = false;
    let mut g = 1.0_f32;
    for i in 0..n {
        let l = look[i];
        if l < g {
            g = l;
        } else {
            g += (1.0 - rel) * (l - g); // release toward l, never above it
        }
        if g < 0.9999 {
            engaged = true;
        }
        left[i] *= g;
        right[i] *= g;
    }
    engaged
}

/// Apply raised-cosine fades to the start and end, so the export opens and closes
/// gently (no transient at the top, no hard-cut of the sustained drone at the end —
/// the reverb tail decays into the fade).
fn apply_fades(left: &mut [f32], right: &mut [f32], sr: f32, fade_in_s: f32, fade_out_s: f32) {
    let n = left.len();
    let fi = ((fade_in_s * sr) as usize).min(n);
    for i in 0..fi {
        let g = 0.5 - 0.5 * (PI * i as f32 / fi as f32).cos();
        left[i] *= g;
        right[i] *= g;
    }
    let fo = ((fade_out_s * sr) as usize).min(n);
    for i in 0..fo {
        let g = 0.5 - 0.5 * (PI * i as f32 / fo as f32).cos();
        let idx = n - 1 - i;
        left[idx] *= g;
        right[idx] *= g;
    }
}

/// RMS level of a buffer.
fn rms_of(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|&x| (x as f64) * (x as f64)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}

/// Correct a left/right level imbalance so the image sits centred, bringing both
/// channels toward their mean RMS. Capped at ±3 dB so a deliberately panned mix
/// isn't flattened.
fn balance_stereo(left: &mut [f32], right: &mut [f32]) {
    let (rl, rr) = (rms_of(left), rms_of(right));
    if rl < 1e-6 || rr < 1e-6 {
        return;
    }
    let mean = 0.5 * (rl + rr);
    let cap = db_to_lin(3.0);
    let gl = (mean / rl).clamp(1.0 / cap, cap);
    let gr = (mean / rr).clamp(1.0 / cap, cap);
    for v in left.iter_mut() {
        *v *= gl;
    }
    for v in right.iter_mut() {
        *v *= gr;
    }
}

/// Stereo (Pearson) correlation, -1..1.
fn correlation(left: &[f32], right: &[f32]) -> f32 {
    let (mut lr, mut ll, mut rr) = (0.0_f64, 0.0, 0.0);
    for i in 0..left.len() {
        lr += (left[i] as f64) * (right[i] as f64);
        ll += (left[i] as f64) * (left[i] as f64);
        rr += (right[i] as f64) * (right[i] as f64);
    }
    let denom = (ll * rr).sqrt();
    if denom < 1e-12 {
        1.0
    } else {
        (lr / denom) as f32
    }
}

/// High-band vs low-band energy balance (dB): a rough tonal-tilt indicator.
fn spectral_tilt_db(left: &[f32], right: &[f32], sr: f32) -> f32 {
    let lp = rbj_lowpass(sr, 250.0, 0.707);
    let hp = rbj_highpass(sr, 4000.0, 0.707);
    let low = lp.mean_square(left) + lp.mean_square(right);
    let high = hp.mean_square(left) + hp.mean_square(right);
    10.0 * (high.max(1e-12) / low.max(1e-12)).log10() as f32
}

/// Master a stereo mixdown: returns the mastered (left, right) and a [`MasterReport`].
pub fn master(
    left: &[f32],
    right: &[f32],
    s: &MasterSettings,
) -> (Vec<f32>, Vec<f32>, MasterReport) {
    let sr = s.sample_rate as f32;
    let mut l = left.to_vec();
    let mut r = right.to_vec();
    let n = l.len().min(r.len());
    l.truncate(n);
    r.truncate(n);

    // 0. Centre the stereo image: correct any left/right level imbalance (e.g. from a
    // camera-orbit pan that didn't average out over a short take). Capped so a
    // genuinely panned mix isn't forced to mono.
    balance_stereo(&mut l, &mut r);
    // 1. Subsonic high-pass (~36 Hz) clears the very low energy that common
    // playback systems reproduce poorly and that mostly spends headroom on rumble.
    let hp = rbj_highpass(sr, 36.0, 0.707);
    hp.process(&mut l);
    hp.process(&mut r);
    // 2. Mono-sum the deep bass for translation on phones / mono / single-speaker.
    mono_bass(&mut l, &mut r, sr, 150.0);

    // 3. Measure, 4. gain to target.
    let lufs_in = integrated_lufs(&l, &r, s.sample_rate);
    let gain_db = if lufs_in.is_finite() {
        (s.target_lufs - lufs_in).clamp(-24.0, 24.0)
    } else {
        0.0
    };
    let gain = db_to_lin(gain_db);
    for i in 0..n {
        l[i] *= gain;
        r[i] *= gain;
    }
    // Gentle fades so the piece opens and resolves cleanly.
    apply_fades(&mut l, &mut r, sr, 0.5, 3.0);

    // 5. Limit to the true-peak ceiling. Limit to ~1 dB under the ceiling at sample
    // rate to leave room for inter-sample peaks, then guarantee with a true-peak trim.
    let ceiling = db_to_lin(s.true_peak_ceiling_db);
    let limited = limit(&mut l, &mut r, sr, ceiling * db_to_lin(-1.0));
    let tp = TruePeak::new();
    let true_peak = tp.peak(&l).max(tp.peak(&r));
    if true_peak > ceiling {
        let trim = ceiling / true_peak;
        for i in 0..n {
            l[i] *= trim;
            r[i] *= trim;
        }
    }

    // Final measurements for the report.
    let final_tp = tp.peak(&l).max(tp.peak(&r));
    let sample_peak = l
        .iter()
        .chain(r.iter())
        .fold(0.0_f32, |m, &x| m.max(x.abs()));
    let report = MasterReport {
        duration_secs: n as f32 / sr,
        lufs_in,
        lufs_out: integrated_lufs(&l, &r, s.sample_rate),
        true_peak_db: lin_to_db(final_tp),
        sample_peak_db: lin_to_db(sample_peak),
        gain_db,
        limited,
        stereo_correlation: correlation(&l, &r),
        spectral_tilt_db: spectral_tilt_db(&l, &r, sr),
    };
    (l, r, report)
}

/// Encode planar stereo `f32` (-1..1) to a 24-bit PCM WAV byte stream.
pub fn encode_wav_24(left: &[f32], right: &[f32], sample_rate: u32) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let channels = 2u16;
    let bits = 24u16;
    let block_align = channels * bits / 8; // 6 bytes/frame
    let byte_rate = sample_rate * block_align as u32;
    let data_len = frames as u32 * block_align as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());

    let write_sample = |out: &mut Vec<u8>, x: f32| {
        let v = (x.clamp(-1.0, 0.999_999_9) * 8_388_607.0).round() as i32;
        let u = (v & 0x00FF_FFFF) as u32; // 24-bit two's-complement
        out.push((u & 0xFF) as u8);
        out.push(((u >> 8) & 0xFF) as u8);
        out.push(((u >> 16) & 0xFF) as u8);
    };
    for i in 0..frames {
        write_sample(&mut out, left[i]);
        write_sample(&mut out, right[i]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stereo sine of the given amplitude and frequency.
    fn sine(amp: f32, freq: f32, sr: u32, secs: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (sr as f32 * secs) as usize;
        let ch: Vec<f32> = (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / sr as f32).sin())
            .collect();
        (ch.clone(), ch)
    }

    #[test]
    fn wav_header_and_size_are_valid() {
        let (l, r) = sine(0.5, 220.0, 48_000, 0.05);
        let wav = encode_wav_24(&l, &r, 48_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2); // channels
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            48_000
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 24); // bits
        assert_eq!(&wav[36..40], b"data");
        let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
        assert_eq!(data_len, l.len() * 6);
        assert_eq!(wav.len(), 44 + data_len);
    }

    #[test]
    fn louder_input_reads_higher_lufs() {
        let sr = 48_000;
        let quiet = sine(0.1, 1_000.0, sr, 1.0);
        let loud = sine(0.4, 1_000.0, sr, 1.0);
        let lq = integrated_lufs(&quiet.0, &quiet.1, sr);
        let ll = integrated_lufs(&loud.0, &loud.1, sr);
        assert!(
            ll > lq + 6.0,
            "4× amplitude should be ~+12 LU: {lq} -> {ll}"
        );
    }

    #[test]
    fn doubling_amplitude_adds_about_6_lu() {
        let sr = 48_000;
        let a = sine(0.2, 1_000.0, sr, 1.0);
        let b = sine(0.4, 1_000.0, sr, 1.0);
        let la = integrated_lufs(&a.0, &a.1, sr);
        let lb = integrated_lufs(&b.0, &b.1, sr);
        assert!(
            (lb - la - 6.02).abs() < 0.3,
            "expected ~+6 LU, got {}",
            lb - la
        );
    }

    #[test]
    fn master_hits_target_lufs() {
        let sr = 48_000;
        // A moderately complex signal (two partials) at an arbitrary level. 20 s, so
        // the fixed start/end fades are a negligible fraction (as in a real export).
        let n = sr as usize * 20;
        let ch: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr as f32;
                0.15 * (2.0 * PI * 110.0 * t).sin() + 0.1 * (2.0 * PI * 220.0 * t).sin()
            })
            .collect();
        let s = MasterSettings {
            sample_rate: sr,
            target_lufs: -16.0,
            true_peak_ceiling_db: -1.0,
        };
        let (_, _, rep) = master(&ch, &ch, &s);
        assert!(
            (rep.lufs_out - (-16.0)).abs() < 1.0,
            "should land near -16 LUFS, got {}",
            rep.lufs_out
        );
    }

    #[test]
    fn true_peak_never_exceeds_ceiling() {
        let sr = 48_000;
        // High crest factor: a quiet bed (low integrated loudness) with near-full-scale
        // transients. Normalising the quiet average up sends the transients well past
        // the ceiling, so the limiter must engage and the true-peak trim must hold.
        let n = sr as usize / 2;
        let mut l: Vec<f32> = (0..n)
            .map(|i| 0.03 * (2.0 * PI * 200.0 * i as f32 / sr as f32).sin())
            .collect();
        for k in (0..n).step_by(4_800) {
            l[k] = 0.98;
        }
        let r = l.clone();
        let s = MasterSettings {
            sample_rate: sr,
            target_lufs: -16.0,
            true_peak_ceiling_db: -1.0,
        };
        let (_, _, rep) = master(&l, &r, &s);
        assert!(
            rep.true_peak_db <= -1.0 + 0.05,
            "true peak {} exceeded the -1 dBTP ceiling",
            rep.true_peak_db
        );
        assert!(rep.limited, "limiter should have engaged on a hot signal");
    }

    #[test]
    fn mono_bass_makes_the_low_end_coherent() {
        // Two orthogonal (decorrelated) low sines: after mono-bass their low bands
        // should carry the same mono signal, i.e. become coherent.
        let sr = 48_000;
        let n = sr as usize / 2;
        let mut l: Vec<f32> = (0..n)
            .map(|i| 0.5 * (2.0 * PI * 40.0 * i as f32 / sr as f32).sin())
            .collect();
        let mut r: Vec<f32> = (0..n)
            .map(|i| 0.5 * (2.0 * PI * 40.0 * i as f32 / sr as f32).cos())
            .collect();
        assert!(
            correlation(&l, &r).abs() < 0.2,
            "inputs should be ~decorrelated"
        );
        mono_bass(&mut l, &mut r, sr as f32, 120.0);
        let lp = rbj_lowpass(sr as f32, 120.0, 0.707);
        let mut ll = l.clone();
        let mut rr = r.clone();
        lp.process(&mut ll);
        lp.process(&mut rr);
        assert!(
            correlation(&ll, &rr) > 0.9,
            "low band should be mono after mono-bass, got {}",
            correlation(&ll, &rr)
        );
    }

    #[test]
    fn silence_masters_without_panicking() {
        let z = vec![0.0_f32; 48_000];
        let s = MasterSettings::default();
        let (l, _, rep) = master(&z, &z, &s);
        assert_eq!(l.len(), z.len());
        assert!(!rep.lufs_in.is_finite()); // silence has no defined loudness
        assert!(rep.true_peak_db < -60.0);
    }

    #[test]
    fn wav_roundtrips_a_known_sample() {
        // A positive half-scale sample encodes to ~+0x400000 in 24-bit.
        let l = vec![0.5_f32];
        let r = vec![0.0_f32];
        let wav = encode_wav_24(&l, &r, 48_000);
        let base = 44;
        let v = (wav[base] as u32) | ((wav[base + 1] as u32) << 8) | ((wav[base + 2] as u32) << 16);
        let expected = (0.5_f32 * 8_388_607.0).round() as u32;
        assert_eq!(v, expected);
    }
}
