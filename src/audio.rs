use crate::constants::{
    DIST_NORM_DIVISOR, D_SEND_BASE, D_SEND_CLAMP_MAX, D_SEND_SPAN, LEVEL_BASE, LEVEL_SPAN,
    MAX_POLYPHONY, R_SEND_BASE, R_SEND_CLAMP_MAX, R_SEND_SPAN, SEND_BOOST_COEFF,
};
use crate::core::{Hz, SculptureAudio, Waveform, EXPORT_TAIL_SEC};
use glam::Vec3;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

// Per-note amplitude envelope: a short linear attack into an exponential release tail,
// which reads softer and more "ambient" than a linear ramp to zero.
const NOTE_ATTACK_SEC: f64 = 0.18;
const NOTE_RELEASE_TAU_FRAC: f64 = 0.56; // release time constant = duration * this
const NOTE_RELEASE_STOP_MULT: f64 = 2.25; // stop the oscillator after attack + duration * this

// Gentle per-voice low-pass on the dry path warms the raw oscillators (the reverb/delay
// sends are already dark), taming the sawtooth's fizz without dulling the sine/triangle.
const VOICE_TONE_CUTOFF_HZ: f32 = 1450.0;
const TONAL_MID_DIP_HZ: f32 = 760.0;
const TONAL_MID_DIP_Q: f32 = 0.72;
const TONAL_MID_DIP_DB: f32 = -10.0;
const STAR_VOICES: usize = 5;
const STAR_MULT: [f32; STAR_VOICES] = [4.0, 6.0, 8.0, 12.0, 16.0];
const PAD_DRIFT_LFO_HZ: [f32; 3] = [0.019, 0.031, 0.047];
const PAD_DRIFT_SCALE: [f32; 3] = [0.72, 1.0, 1.24];

thread_local! {
    static MASTER_UNMUTED_GAIN: RefCell<Option<f32>> = const { RefCell::new(None) };
}

/// Toggle the master bus between muted and its previous gain (remembered across
/// toggles). Returns the new muted state.
pub fn toggle_master_mute(master_gain: &web::GainNode) -> bool {
    let current = master_gain.gain().value();
    if current <= 0.0001 {
        let restored = MASTER_UNMUTED_GAIN
            .with(|s| s.borrow_mut().take())
            .unwrap_or(0.25)
            .clamp(0.0, 1.0);
        master_gain.gain().set_value(restored);
        false
    } else {
        MASTER_UNMUTED_GAIN.with(|s| *s.borrow_mut() = Some(current));
        master_gain.gain().set_value(0.0);
        true
    }
}

pub struct FxBuses {
    pub master_gain: web::GainNode,
    pub sat_pre: web::GainNode,
    pub sat_wet: web::GainNode,
    pub sat_dry: web::GainNode,
    pub reverb_in: web::GainNode,
    pub reverb_wet: web::GainNode,
    pub delay_in: web::GainNode,
    pub delay_feedback: web::GainNode,
    pub delay_wet: web::GainNode,
    pub sculpture: SculptureLayer,
}

pub struct VoiceRouting {
    pub voice_gains: Vec<web::GainNode>,
    pub voice_panners: Vec<web::PannerNode>,
    pub delay_sends: Vec<web::GainNode>,
    pub reverb_sends: Vec<web::GainNode>,
}

#[derive(Clone)]
pub struct SculptureLayer {
    pub pad_oscs: Vec<web::OscillatorNode>,
    pad_drift_depths: Vec<web::GainNode>,
    _pad_drift_lfos: Vec<web::OscillatorNode>,
    pub pad_gain: web::GainNode,
    pub pad_lp: web::BiquadFilterNode,
    pub field_pan: web::StereoPannerNode,
    pub sub_osc: web::OscillatorNode,
    pub sub_gain: web::GainNode,
    pub star_oscs: Vec<web::OscillatorNode>,
    pub star_lp: web::BiquadFilterNode,
    pub star_gain: web::GainNode,
    pub shimmer_gain: web::GainNode,
    pub noise_gain: web::GainNode,
    pub _holds: Vec<web::AudioBufferSourceNode>,
}

fn create_gain(
    audio_ctx: &web::BaseAudioContext,
    value: f32,
    label: &str,
) -> anyhow::Result<web::GainNode> {
    let g =
        web::GainNode::new(audio_ctx).map_err(|e| anyhow::anyhow!("{label} GainNode: {e:?}"))?;
    g.gain().set_value(value);
    Ok(g)
}

pub fn build_fx_buses(audio_ctx: &web::AudioContext) -> anyhow::Result<FxBuses> {
    build_fx_buses_on(audio_ctx.as_ref())
}

fn build_sculpture_layer(
    ctx: &web::BaseAudioContext,
    master_gain: &web::GainNode,
    reverb_in: &web::GainNode,
) -> anyhow::Result<SculptureLayer> {
    let field_pan =
        web::StereoPannerNode::new(ctx).map_err(|e| anyhow::anyhow!("StereoPannerNode: {e:?}"))?;
    _ = field_pan.connect_with_audio_node(master_gain);
    _ = field_pan.connect_with_audio_node(reverb_in);

    let pad_gain = create_gain(ctx, 0.0, "pad gain")?;
    let pad_lp = lowpass(ctx, 620.0, 0.7, "pad lowpass")?;
    let pad_mid_dip = tonal_mid_dip(ctx, "pad mid dip")?;
    _ = pad_gain.connect_with_audio_node(&pad_lp);
    _ = pad_lp.connect_with_audio_node(&pad_mid_dip);
    _ = pad_mid_dip.connect_with_audio_node(&field_pan);

    let mut pad_oscs = Vec::with_capacity(3);
    let mut pad_drift_depths = Vec::with_capacity(3);
    let mut pad_drift_lfos = Vec::with_capacity(3);
    for i in 0..3 {
        let osc = web::OscillatorNode::new(ctx)
            .map_err(|e| anyhow::anyhow!("pad OscillatorNode: {e:?}"))?;
        osc.set_type(if i == 0 {
            web::OscillatorType::Sine
        } else {
            web::OscillatorType::Triangle
        });
        osc.frequency().set_value(110.0);
        osc.detune().set_value((i as f32 - 1.0) * 4.0);
        let drift = web::OscillatorNode::new(ctx)
            .map_err(|e| anyhow::anyhow!("pad drift OscillatorNode: {e:?}"))?;
        drift.set_type(web::OscillatorType::Sine);
        drift.frequency().set_value(PAD_DRIFT_LFO_HZ[i]);
        let drift_depth = create_gain(ctx, 0.0, "pad drift depth")?;
        _ = drift.connect_with_audio_node(&drift_depth);
        connect_param(&drift_depth, &osc.detune());
        _ = drift.start();
        _ = osc.connect_with_audio_node(&pad_gain);
        _ = osc.start();
        pad_oscs.push(osc);
        pad_drift_depths.push(drift_depth);
        pad_drift_lfos.push(drift);
    }

    let shimmer = octave_up_shaper(ctx, "shimmer shaper")?;
    let shimmer_band = bandpass(ctx, 560.0, 0.32, "shimmer bandpass")?;
    let shimmer_mid_dip = tonal_mid_dip(ctx, "shimmer mid dip")?;
    let shimmer_gain = create_gain(ctx, 0.0, "shimmer gain")?;
    _ = pad_mid_dip.connect_with_audio_node(&shimmer);
    _ = shimmer.connect_with_audio_node(&shimmer_band);
    _ = shimmer_band.connect_with_audio_node(&shimmer_mid_dip);
    _ = shimmer_mid_dip.connect_with_audio_node(&shimmer_gain);
    _ = shimmer_gain.connect_with_audio_node(reverb_in);

    let sub_osc =
        web::OscillatorNode::new(ctx).map_err(|e| anyhow::anyhow!("sub OscillatorNode: {e:?}"))?;
    sub_osc.set_type(web::OscillatorType::Sine);
    sub_osc.frequency().set_value(55.0);
    let sub_lp = lowpass(ctx, 120.0, 0.5, "sub lowpass")?;
    let sub_gain = create_gain(ctx, 0.0, "sub gain")?;
    _ = sub_osc.connect_with_audio_node(&sub_lp);
    _ = sub_lp.connect_with_audio_node(&sub_gain);
    _ = sub_gain.connect_with_audio_node(master_gain);
    _ = sub_osc.start();

    let star_lp = lowpass(ctx, 820.0, 0.50, "star lowpass")?;
    let star_mid_dip = tonal_mid_dip(ctx, "star mid dip")?;
    let star_gain = create_gain(ctx, 0.0, "star gain")?;
    _ = star_lp.connect_with_audio_node(&star_mid_dip);
    _ = star_mid_dip.connect_with_audio_node(&star_gain);
    _ = star_gain.connect_with_audio_node(&field_pan);
    let mut star_oscs = Vec::with_capacity(STAR_VOICES);
    for i in 0..STAR_VOICES {
        let osc = web::OscillatorNode::new(ctx)
            .map_err(|e| anyhow::anyhow!("star OscillatorNode: {e:?}"))?;
        osc.set_type(web::OscillatorType::Sine);
        osc.frequency().set_value(360.0 + 70.0 * i as f32);
        osc.detune().set_value((i as f32 - 2.0) * 3.0);
        let voice_gain = create_gain(ctx, 0.26, "star voice gain")?;
        _ = osc.connect_with_audio_node(&voice_gain);
        _ = voice_gain.connect_with_audio_node(&star_lp);
        _ = osc.start();
        star_oscs.push(osc);
    }

    let noise_src = web::AudioBufferSourceNode::new(ctx)
        .map_err(|e| anyhow::anyhow!("noise AudioBufferSourceNode: {e:?}"))?;
    if let Some(buf) = make_noise_buffer(ctx, 4.0) {
        noise_src.set_buffer(Some(&buf));
    }
    noise_src.set_loop(true);
    let noise_lp = lowpass(ctx, 360.0, 0.45, "noise lowpass")?;
    let noise_gain = create_gain(ctx, 0.0, "noise gain")?;
    _ = noise_src.connect_with_audio_node(&noise_lp);
    _ = noise_lp.connect_with_audio_node(&noise_gain);
    _ = noise_gain.connect_with_audio_node(master_gain);
    _ = noise_src.start();

    Ok(SculptureLayer {
        pad_oscs,
        pad_drift_depths,
        _pad_drift_lfos: pad_drift_lfos,
        pad_gain,
        pad_lp,
        field_pan,
        sub_osc,
        sub_gain,
        star_oscs,
        star_lp,
        star_gain,
        shimmer_gain,
        noise_gain,
        _holds: vec![noise_src],
    })
}

/// A scheduled note's nodes, retained so they can be disconnected once it has
/// finished (rather than left for the garbage collector).
pub struct ActiveNote {
    pub osc: web::OscillatorNode,
    pub gain: web::GainNode,
    pub stop_time: f64,
}

/// Fire a one-shot oscillator routed through a voice's gain and sends. Returns the
/// created nodes (and the time they stop) so the caller can disconnect them later.
#[allow(clippy::too_many_arguments)]
fn trigger_one_shot(
    audio_ctx: &web::BaseAudioContext,
    waveform: Waveform,
    freq: Hz,
    velocity: f32,
    duration_sec: f64,
    at_time: f64,
    voice_gain: &web::GainNode,
    delay_send: &web::GainNode,
    reverb_send: &web::GainNode,
) -> Option<ActiveNote> {
    let src = web::OscillatorNode::new(audio_ctx).ok()?;
    match waveform {
        Waveform::Sine => src.set_type(web::OscillatorType::Sine),
        Waveform::Saw => src.set_type(web::OscillatorType::Sawtooth),
        Waveform::Triangle => src.set_type(web::OscillatorType::Triangle),
    }
    src.frequency().set_value(freq.0);
    let gain = web::GainNode::new(audio_ctx).ok()?;
    // Anchor the envelope at `at_time` (the scheduled start) so the attack fires at the
    // right moment, not from the AudioContext's current time.
    gain.gain().set_value(0.0);
    let t0 = at_time;
    let attack_end = t0 + NOTE_ATTACK_SEC;
    _ = gain.gain().set_value_at_time(0.0, t0);
    _ = gain
        .gain()
        .linear_ramp_to_value_at_time(velocity, attack_end);
    // Exponential decay toward silence; it never quite reaches 0, so the oscillator is
    // stopped (below) once the tail is inaudible.
    let release_tau = (duration_sec * NOTE_RELEASE_TAU_FRAC).max(0.04);
    _ = gain.gain().set_target_at_time(0.0, attack_end, release_tau);
    _ = src.connect_with_audio_node(&gain);
    _ = gain.connect_with_audio_node(voice_gain);
    _ = gain.connect_with_audio_node(delay_send);
    _ = gain.connect_with_audio_node(reverb_send);
    _ = src.start_with_when(t0);
    let stop_time = attack_end + duration_sec * NOTE_RELEASE_STOP_MULT;
    _ = src.stop_with_when(stop_time);
    Some(ActiveNote {
        osc: src,
        gain,
        stop_time,
    })
}

/// Spawn a note into `active_notes`, reaping finished notes and enforcing the
/// polyphony cap first. Shared by the audio scheduler (rhythmic notes) and the
/// frame loop (click-to-play). `at_time` is the AudioContext time to start at.
#[allow(clippy::too_many_arguments)]
pub fn spawn_note(
    audio_ctx: &web::AudioContext,
    waveform: Waveform,
    freq: Hz,
    velocity: f32,
    duration_sec: f64,
    at_time: f64,
    voice_gain: &web::GainNode,
    delay_send: &web::GainNode,
    reverb_send: &web::GainNode,
    active_notes: &RefCell<Vec<ActiveNote>>,
) {
    let now = audio_ctx.current_time();
    let base: &web::BaseAudioContext = audio_ctx.as_ref();
    {
        let mut notes = active_notes.borrow_mut();
        notes.retain(|n| {
            if n.stop_time <= now {
                _ = n.osc.disconnect();
                _ = n.gain.disconnect();
                false
            } else {
                true
            }
        });
        if notes.len() >= MAX_POLYPHONY {
            return;
        }
    }
    if let Some(note) = trigger_one_shot(
        base,
        waveform,
        freq,
        velocity,
        duration_sec,
        at_time,
        voice_gain,
        delay_send,
        reverb_send,
    ) {
        active_notes.borrow_mut().push(note);
    }
}

pub fn apply_sculpture_fx(fx: &FxBuses, target: &SculptureAudio, on: bool, now: f64) {
    apply_sculpture_layer(&fx.sculpture, target, on, now);
    apply_sculpture_bus(
        &fx.reverb_wet,
        &fx.delay_wet,
        &fx.delay_feedback,
        &fx.sat_pre,
        &fx.sat_wet,
        &fx.sat_dry,
        target,
        now,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn apply_sculpture_bus(
    reverb_wet: &web::GainNode,
    delay_wet: &web::GainNode,
    delay_feedback: &web::GainNode,
    sat_pre: &web::GainNode,
    sat_wet: &web::GainNode,
    sat_dry: &web::GainNode,
    target: &SculptureAudio,
    now: f64,
) {
    ramp(&reverb_wet.gain(), target.reverb_wet, now, 0.45);
    ramp(&delay_wet.gain(), target.delay_wet, now, 0.30);
    ramp(&delay_feedback.gain(), target.delay_feedback, now, 0.35);
    ramp(&sat_pre.gain(), target.analog_drive, now, 0.70);
    let wet = target.analog_wet.clamp(0.0, 1.0);
    ramp(&sat_wet.gain(), wet, now, 0.55);
    ramp(&sat_dry.gain(), 1.0 - wet, now, 0.55);
}

pub fn apply_sculpture_layer(layer: &SculptureLayer, target: &SculptureAudio, on: bool, now: f64) {
    for (i, osc) in layer.pad_oscs.iter().enumerate() {
        ramp(&osc.frequency(), target.pad_freqs[i], now, 0.35);
        ramp(
            &osc.detune(),
            (i as f32 - 1.0) * target.pad_detune_cents,
            now,
            0.50,
        );
    }
    for (i, depth) in layer.pad_drift_depths.iter().enumerate() {
        let cents = if on {
            target.analog_drift_cents * PAD_DRIFT_SCALE[i]
        } else {
            0.0
        };
        ramp(&depth.gain(), cents, now, 0.90);
    }
    ramp(
        &layer.pad_gain.gain(),
        if on { target.pad_gain } else { 0.0 },
        now,
        0.80,
    );
    ramp(&layer.pad_lp.frequency(), target.pad_cutoff_hz, now, 0.45);
    ramp(&layer.sub_osc.frequency(), target.sub_freq, now, 0.40);
    ramp(
        &layer.sub_gain.gain(),
        if on { target.sub_gain } else { 0.0 },
        now,
        0.70,
    );
    for (osc, mult) in layer.star_oscs.iter().zip(STAR_MULT) {
        ramp(
            &osc.frequency(),
            (target.pad_freqs[0] * mult).clamp(95.0, 1450.0),
            now,
            0.45,
        );
    }
    ramp(&layer.star_lp.frequency(), target.star_cutoff_hz, now, 0.45);
    ramp(
        &layer.star_gain.gain(),
        if on { target.star_gain } else { 0.0 },
        now,
        0.65,
    );
    ramp(
        &layer.shimmer_gain.gain(),
        if on { target.shimmer_gain } else { 0.0 },
        now,
        0.55,
    );
    ramp(
        &layer.noise_gain.gain(),
        if on { target.noise_gain } else { 0.0 },
        now,
        0.75,
    );
    ramp(&layer.field_pan.pan(), target.field_pan, now, 0.35);
}

/// Create an analyser node and a buffer sized to its frequency-bin count, used to
/// drive ambient visuals from the audio output.
pub fn create_analyser(
    audio_ctx: &web::AudioContext,
) -> (Option<web::AnalyserNode>, Rc<RefCell<Vec<f32>>>) {
    let analyser: Option<web::AnalyserNode> = web::AnalyserNode::new(audio_ctx).ok();
    if let Some(a) = &analyser {
        a.set_fft_size(256);
    }
    let buf: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    if let Some(a) = &analyser {
        let bins = a.frequency_bin_count() as usize;
        buf.borrow_mut().resize(bins, 0.0);
    }
    (analyser, buf)
}

/// Wire each voice's panner (HRTF, inverse-distance), gain, and delay/reverb sends,
/// returning the per-voice nodes the frame loop positions each frame.
pub fn wire_voices(
    audio_ctx: &web::AudioContext,
    initial_positions: &[Vec3],
    master_gain: &web::GainNode,
    delay_in: &web::GainNode,
    reverb_in: &web::GainNode,
) -> anyhow::Result<VoiceRouting> {
    let ctx: &web::BaseAudioContext = audio_ctx.as_ref();
    let mut voice_gains: Vec<web::GainNode> = Vec::new();
    let mut voice_panners: Vec<web::PannerNode> = Vec::new();
    let mut delay_sends_vec: Vec<web::GainNode> = Vec::new();
    let mut reverb_sends_vec: Vec<web::GainNode> = Vec::new();

    for pos in initial_positions.iter() {
        let panner = web::PannerNode::new(ctx).map_err(|e| anyhow::anyhow!("PannerNode: {e:?}"))?;
        panner.set_panning_model(web::PanningModelType::Hrtf);
        panner.set_distance_model(web::DistanceModelType::Inverse);
        panner.set_ref_distance(0.5);
        panner.set_max_distance(50.0);
        panner.position_x().set_value(pos.x);
        panner.position_y().set_value(pos.y);
        panner.position_z().set_value(pos.z);

        let gain = create_gain(ctx, 0.0, "Voice gain")?;
        // Dry path: voice gain -> gentle low-pass -> panner -> master.
        let tone = web::BiquadFilterNode::new(ctx)
            .map_err(|e| anyhow::anyhow!("Voice tone filter: {e:?}"))?;
        tone.set_type(web::BiquadFilterType::Lowpass);
        tone.frequency().set_value(VOICE_TONE_CUTOFF_HZ);
        let mid_dip = tonal_mid_dip(ctx, "Voice mid dip")?;
        _ = gain.connect_with_audio_node(&tone);
        _ = tone.connect_with_audio_node(&mid_dip);
        _ = mid_dip.connect_with_audio_node(&panner);
        _ = panner.connect_with_audio_node(master_gain);

        let d_send = create_gain(ctx, 0.4, "Delay send")?;
        _ = d_send.connect_with_audio_node(delay_in);
        delay_sends_vec.push(d_send);

        let r_send = create_gain(ctx, 0.65, "Reverb send")?;
        _ = r_send.connect_with_audio_node(reverb_in);
        reverb_sends_vec.push(r_send);

        voice_gains.push(gain);
        voice_panners.push(panner);
    }

    Ok(VoiceRouting {
        voice_gains,
        voice_panners,
        delay_sends: delay_sends_vec,
        reverb_sends: reverb_sends_vec,
    })
}

pub async fn render_piece_offline(
    arrangement: &crate::core::PieceArrangement,
    sample_rate: u32,
    render_level: f32,
    note_seed: u64,
) -> Option<(Vec<f32>, Vec<f32>)> {
    let length = ((arrangement.duration + EXPORT_TAIL_SEC) * sample_rate as f64).ceil() as u32;
    let octx = web::OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(
        2,
        length,
        sample_rate as f32,
    )
    .ok()?;
    let ctx: &web::BaseAudioContext = octx.as_ref();
    let fx = build_fx_buses_on(ctx).ok()?;
    fx.master_gain.gain().set_value(render_level);

    let first = arrangement.moment(0.0);
    let routing = wire_voices_on(
        ctx,
        &first.voice_positions,
        &fx.master_gain,
        &fx.delay_in,
        &fx.reverb_in,
    )
    .ok()?;

    let timeline = arrangement.timeline(1.0 / 30.0);
    for (t, moment) in &timeline {
        apply_sculpture_fx(&fx, &moment.audio, true, *t);
        apply_voice_space_at(&routing, &moment.voice_positions, moment.swirl_energy, *t);
    }

    let mut engine = crate::core::MusicEngine::new(
        crate::core::default_voice_configs(),
        crate::core::EngineParams::default(),
        note_seed,
    );
    engine.reseed_all_from(note_seed);

    let mut next = 0.0_f64;
    let mut notes = Vec::new();
    while next < arrangement.duration {
        let moment = arrangement.moment(next);
        moment.apply_to_engine(&mut engine);
        notes.clear();
        engine.step(&mut notes);
        for ev in &notes {
            let waveform = engine.configs[ev.voice.0].waveform;
            let _ = trigger_one_shot(
                ctx,
                waveform,
                ev.freq,
                ev.velocity,
                ev.duration_sec as f64,
                next,
                &routing.voice_gains[ev.voice.0],
                &routing.delay_sends[ev.voice.0],
                &routing.reverb_sends[ev.voice.0],
            );
        }
        next += engine.params.bpm.eighth_step_seconds().max(0.01);
    }

    let rendered = wasm_bindgen_futures::JsFuture::from(octx.start_rendering().ok()?)
        .await
        .ok()?;
    let buffer: web::AudioBuffer = rendered.dyn_into().ok()?;
    let left = buffer.get_channel_data(0).ok()?;
    let right = buffer.get_channel_data(1).ok()?;
    Some((left, right))
}

fn build_fx_buses_on(ctx: &web::BaseAudioContext) -> anyhow::Result<FxBuses> {
    let master_gain = create_gain(ctx, 0.25, "Master")?;

    let sat_pre = create_gain(ctx, 0.9, "sat pre")?;
    let saturator = tape_saturator(ctx, "WaveShaperNode")?;
    let sat_wet = create_gain(ctx, 0.35, "sat wet")?;
    let sat_dry = create_gain(ctx, 0.65, "sat dry")?;
    let output_pre = create_gain(ctx, 1.0, "output pre")?;
    let comp = web::DynamicsCompressorNode::new(ctx)
        .map_err(|e| anyhow::anyhow!("DynamicsCompressorNode: {e:?}"))?;
    comp.threshold().set_value(-20.0);
    comp.knee().set_value(18.0);
    comp.ratio().set_value(2.6);
    comp.attack().set_value(0.006);
    comp.release().set_value(0.32);

    _ = master_gain.connect_with_audio_node(&sat_pre);
    _ = sat_pre.connect_with_audio_node(&saturator);
    _ = saturator.connect_with_audio_node(&sat_wet);
    _ = sat_wet.connect_with_audio_node(&output_pre);
    _ = master_gain.connect_with_audio_node(&sat_dry);
    _ = sat_dry.connect_with_audio_node(&output_pre);
    _ = output_pre.connect_with_audio_node(&comp);
    _ = comp.connect_with_audio_node(&ctx.destination());

    let reverb_in = create_gain(ctx, 1.0, "Reverb in")?;
    let reverb =
        web::ConvolverNode::new(ctx).map_err(|e| anyhow::anyhow!("ConvolverNode: {e:?}"))?;
    reverb.set_normalize(true);
    if let Some(ir) = make_impulse_response(ctx, 7.0) {
        reverb.set_buffer(Some(&ir));
    }
    let reverb_tone = lowpass(ctx, 1050.0, 0.50, "reverb tone")?;
    let reverb_mid_dip = tonal_mid_dip(ctx, "reverb mid dip")?;
    let reverb_wet = create_gain(ctx, 0.62, "Reverb wet")?;
    _ = reverb_in.connect_with_audio_node(&reverb);
    _ = reverb.connect_with_audio_node(&reverb_tone);
    _ = reverb_tone.connect_with_audio_node(&reverb_mid_dip);
    _ = reverb_mid_dip.connect_with_audio_node(&reverb_wet);
    _ = reverb_wet.connect_with_audio_node(&master_gain);

    let delay_in = create_gain(ctx, 1.0, "Delay in")?;
    let delay = ctx
        .create_delay_with_max_delay_time(3.0)
        .map_err(|e| anyhow::anyhow!("DelayNode: {e:?}"))?;
    delay.delay_time().set_value(0.55);
    let delay_tone = lowpass(ctx, 780.0, 0.62, "delay tone")?;
    let delay_mid_dip = tonal_mid_dip(ctx, "delay mid dip")?;
    let delay_feedback = create_gain(ctx, 0.56, "Delay feedback")?;
    let delay_wet = create_gain(ctx, 0.34, "Delay wet")?;
    _ = delay_in.connect_with_audio_node(&delay);
    _ = delay.connect_with_audio_node(&delay_tone);
    _ = delay_tone.connect_with_audio_node(&delay_mid_dip);
    _ = delay_mid_dip.connect_with_audio_node(&delay_feedback);
    _ = delay_feedback.connect_with_audio_node(&delay);
    _ = delay_mid_dip.connect_with_audio_node(&delay_wet);
    _ = delay_wet.connect_with_audio_node(&master_gain);

    let sculpture = build_sculpture_layer(ctx, &master_gain, &reverb_in)?;

    Ok(FxBuses {
        master_gain,
        sat_pre,
        sat_wet,
        sat_dry,
        reverb_in,
        reverb_wet,
        delay_in,
        delay_feedback,
        delay_wet,
        sculpture,
    })
}

fn wire_voices_on(
    ctx: &web::BaseAudioContext,
    initial_positions: &[Vec3],
    master_gain: &web::GainNode,
    delay_in: &web::GainNode,
    reverb_in: &web::GainNode,
) -> anyhow::Result<VoiceRouting> {
    let mut voice_gains: Vec<web::GainNode> = Vec::new();
    let mut voice_panners: Vec<web::PannerNode> = Vec::new();
    let mut delay_sends_vec: Vec<web::GainNode> = Vec::new();
    let mut reverb_sends_vec: Vec<web::GainNode> = Vec::new();

    for pos in initial_positions.iter() {
        let panner = web::PannerNode::new(ctx).map_err(|e| anyhow::anyhow!("PannerNode: {e:?}"))?;
        panner.set_panning_model(web::PanningModelType::Hrtf);
        panner.set_distance_model(web::DistanceModelType::Inverse);
        panner.set_ref_distance(0.5);
        panner.set_max_distance(50.0);
        panner.position_x().set_value(pos.x);
        panner.position_y().set_value(pos.y);
        panner.position_z().set_value(pos.z);

        let gain = create_gain(ctx, 0.0, "Voice gain")?;
        let tone = lowpass(ctx, VOICE_TONE_CUTOFF_HZ, 0.7, "Voice tone filter")?;
        let mid_dip = tonal_mid_dip(ctx, "Voice mid dip")?;
        _ = gain.connect_with_audio_node(&tone);
        _ = tone.connect_with_audio_node(&mid_dip);
        _ = mid_dip.connect_with_audio_node(&panner);
        _ = panner.connect_with_audio_node(master_gain);

        let d_send = create_gain(ctx, 0.4, "Delay send")?;
        _ = d_send.connect_with_audio_node(delay_in);
        delay_sends_vec.push(d_send);

        let r_send = create_gain(ctx, 0.65, "Reverb send")?;
        _ = r_send.connect_with_audio_node(reverb_in);
        reverb_sends_vec.push(r_send);

        voice_gains.push(gain);
        voice_panners.push(panner);
    }

    Ok(VoiceRouting {
        voice_gains,
        voice_panners,
        delay_sends: delay_sends_vec,
        reverb_sends: reverb_sends_vec,
    })
}

fn apply_voice_space_at(
    routing: &VoiceRouting,
    voice_positions: &[Vec3; 3],
    swirl_energy: f32,
    at: f64,
) {
    for (i, pos) in voice_positions.iter().enumerate() {
        if let Some(panner) = routing.voice_panners.get(i) {
            _ = panner.position_x().set_value_at_time(pos.x, at);
            _ = panner.position_y().set_value_at_time(pos.y, at);
            _ = panner.position_z().set_value_at_time(pos.z, at);
        }
        let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
        let boost = 1.0 + SEND_BOOST_COEFF * swirl_energy;
        if let Some(send) = routing.delay_sends.get(i) {
            let d_amt = ((D_SEND_BASE + D_SEND_SPAN * pos.x.abs().min(1.0)) * boost)
                .clamp(0.0, D_SEND_CLAMP_MAX);
            _ = send.gain().set_value_at_time(d_amt, at);
        }
        if let Some(send) = routing.reverb_sends.get(i) {
            let r_amt = ((R_SEND_BASE + R_SEND_SPAN * (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0))
                * boost)
                .clamp(0.0, R_SEND_CLAMP_MAX);
            _ = send.gain().set_value_at_time(r_amt, at);
        }
        if let Some(gain) = routing.voice_gains.get(i) {
            let lvl = LEVEL_BASE + LEVEL_SPAN * (1.0 - (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0));
            _ = gain.gain().set_value_at_time(lvl, at);
        }
    }
}

fn ramp(param: &web::AudioParam, value: f32, now: f64, tau: f32) {
    let _ = param.set_target_at_time(value, now, tau as f64);
}

fn connect_param(node: &web::AudioNode, param: &web::AudioParam) {
    let _ = node.connect_with_audio_param(param);
}

fn lowpass(
    ctx: &web::BaseAudioContext,
    freq: f32,
    q: f32,
    label: &str,
) -> anyhow::Result<web::BiquadFilterNode> {
    let node = web::BiquadFilterNode::new(ctx).map_err(|e| anyhow::anyhow!("{label}: {e:?}"))?;
    node.set_type(web::BiquadFilterType::Lowpass);
    node.frequency().set_value(freq);
    node.q().set_value(q);
    Ok(node)
}

fn bandpass(
    ctx: &web::BaseAudioContext,
    freq: f32,
    q: f32,
    label: &str,
) -> anyhow::Result<web::BiquadFilterNode> {
    let node = web::BiquadFilterNode::new(ctx).map_err(|e| anyhow::anyhow!("{label}: {e:?}"))?;
    node.set_type(web::BiquadFilterType::Bandpass);
    node.frequency().set_value(freq);
    node.q().set_value(q);
    Ok(node)
}

fn tonal_mid_dip(
    ctx: &web::BaseAudioContext,
    label: &str,
) -> anyhow::Result<web::BiquadFilterNode> {
    peaking(
        ctx,
        TONAL_MID_DIP_HZ,
        TONAL_MID_DIP_Q,
        TONAL_MID_DIP_DB,
        label,
    )
}

fn peaking(
    ctx: &web::BaseAudioContext,
    freq: f32,
    q: f32,
    gain_db: f32,
    label: &str,
) -> anyhow::Result<web::BiquadFilterNode> {
    let node = web::BiquadFilterNode::new(ctx).map_err(|e| anyhow::anyhow!("{label}: {e:?}"))?;
    node.set_type(web::BiquadFilterType::Peaking);
    node.frequency().set_value(freq);
    node.q().set_value(q);
    node.gain().set_value(gain_db);
    Ok(node)
}

fn tape_saturator(ctx: &web::BaseAudioContext, label: &str) -> anyhow::Result<web::WaveShaperNode> {
    let shaper = web::WaveShaperNode::new(ctx).map_err(|e| anyhow::anyhow!("{label}: {e:?}"))?;
    let mut curve = vec![0.0_f32; 2048];
    let drive = 2.0_f32;
    let last = curve.len() as f32 - 1.0;
    for (i, c) in curve.iter_mut().enumerate() {
        let x = (i as f32 / last) * 2.0 - 1.0;
        *c = (drive * x).tanh() / drive;
    }
    shaper.set_curve_opt_f32_slice(Some(&mut curve));
    shaper.set_oversample(web::OverSampleType::N4x);
    Ok(shaper)
}

fn octave_up_shaper(
    ctx: &web::BaseAudioContext,
    label: &str,
) -> anyhow::Result<web::WaveShaperNode> {
    let shaper = web::WaveShaperNode::new(ctx).map_err(|e| anyhow::anyhow!("{label}: {e:?}"))?;
    let mut curve = vec![0.0_f32; 1024];
    let last = curve.len() as f32 - 1.0;
    for (i, c) in curve.iter_mut().enumerate() {
        let x = -1.0 + 2.0 * i as f32 / last;
        *c = 2.0 * x * x - 1.0;
    }
    shaper.set_curve_opt_f32_slice(Some(&mut curve));
    shaper.set_oversample(web::OverSampleType::N4x);
    Ok(shaper)
}

fn make_noise_buffer(ctx: &web::BaseAudioContext, seconds: f32) -> Option<web::AudioBuffer> {
    let sr = ctx.sample_rate();
    let len = (sr * seconds) as u32;
    let buf = ctx.create_buffer(2, len, sr).ok()?;
    for (ch, seed) in [0x2545_F491u32, 0x9E37_79B9].into_iter().enumerate() {
        let mut brown = 0.0_f32;
        let mut dc = 0.0_f32;
        let mut peak = 1e-6_f32;
        let mut data = Vec::with_capacity(len as usize);
        for white in Noise(seed).take(len as usize) {
            brown = (0.9965 * brown + 0.035 * white).clamp(-1.0, 1.0);
            dc += 0.00035 * (brown - dc);
            let sample = brown - dc;
            peak = peak.max(sample.abs());
            data.push(sample);
        }
        let gain = 0.58 / peak;
        for sample in &mut data {
            *sample *= gain;
        }
        let _ = buf.copy_to_channel(&data, ch as i32);
    }
    Some(buf)
}

fn make_impulse_response(ctx: &web::BaseAudioContext, seconds: f32) -> Option<web::AudioBuffer> {
    let sr = ctx.sample_rate();
    let len = (sr * seconds) as u32;
    let ir = ctx.create_buffer(2, len, sr).ok()?;
    let dt = 1.0 / sr;
    for (ch, seed, jitter) in [(0usize, 0x1234_ABCDu32, 0.0_f32), (1, 0x7890_FEDC, 1.0)] {
        let mut buf = vec![0.0_f32; len as usize];
        let mut lp = 0.0_f32;
        for (i, n) in Noise(seed).take(len as usize).enumerate() {
            let t = i as f32 * dt;
            let cutoff = 0.04 + 0.5 * (-t / 1.1).exp();
            lp += cutoff * (n - lp);
            let decay = (-t / 3.2).exp();
            let onset = 1.0 - (-t / 0.015).exp();
            buf[i] = lp * decay * onset;
        }
        for (k, (time, gain)) in [
            (0.007, 0.70),
            (0.013, 0.55),
            (0.019, 0.62),
            (0.027, 0.45),
            (0.037, 0.50),
            (0.049, 0.38),
            (0.063, 0.42),
            (0.079, 0.30),
            (0.097, 0.33),
            (0.119, 0.24),
            (0.143, 0.26),
            (0.171, 0.19),
        ]
        .iter()
        .enumerate()
        {
            let idx = ((time + jitter * 0.0017 * (k as f32 + 1.0)) * sr) as usize;
            if idx < buf.len() {
                buf[idx] += gain * if k % 2 == 0 { 1.0 } else { -1.0 };
            }
        }
        let _ = ir.copy_to_channel(&buf, ch as i32);
    }
    Some(ir)
}

struct Noise(u32);

impl Iterator for Noise {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        Some(self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0)
    }
}
