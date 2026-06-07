use crate::audio;
use crate::constants::*;
use crate::core::{Bpm, MusicEngine, NoteEvent, VoiceIndex, C_MAJOR_PENTATONIC};
use crate::events::InputCommand;
use crate::input;
use crate::overlay;
use crate::render;
use glam::Vec3;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys as web;
use web_time::Instant;

use crate::constants::CAMERA_Z;

pub struct FrameContext<'a> {
    pub engine: Rc<RefCell<MusicEngine>>,
    pub paused: Rc<RefCell<bool>>,
    pub input_queue: Rc<RefCell<VecDeque<InputCommand>>>,
    pub pulses: Rc<RefCell<Vec<f32>>>,
    #[allow(dead_code)] // Used in pointer events, not directly in frame module
    pub hover_index: Rc<RefCell<Option<usize>>>,

    pub canvas: web::HtmlCanvasElement,
    pub mouse: Rc<RefCell<input::MouseState>>,

    pub audio_ctx: web::AudioContext,
    pub master_gain: web::GainNode,
    pub listener: web::AudioListener,
    pub voice_gains: Rc<Vec<web::GainNode>>,
    pub delay_sends: Rc<Vec<web::GainNode>>,
    pub reverb_sends: Rc<Vec<web::GainNode>>,
    pub voice_panners: Vec<web::PannerNode>,

    pub reverb_wet: web::GainNode,
    pub delay_wet: web::GainNode,
    pub delay_feedback: web::GainNode,
    pub sat_pre: web::GainNode,
    pub sat_wet: web::GainNode,
    pub sat_dry: web::GainNode,

    pub analyser: Option<web::AnalyserNode>,
    pub analyser_buf: Rc<RefCell<Vec<f32>>>,

    pub gpu: Option<render::GpuState<'a>>,
    pub pending_ripple: Option<[f32; 2]>,

    pub last_instant: Instant,
    pub prev_uv: [f32; 2],
    pub swirl_energy: f32,
    pub swirl_pos: [f32; 2],
    pub swirl_vel: [f32; 2],
    pub swirl_initialized: bool,
    pub pulse_energy: [f32; 3],
}

impl<'a> FrameContext<'a> {
    pub fn frame(&mut self) {
        let now = Instant::now();
        let dt = now - self.last_instant;
        self.last_instant = now;
        let dt_sec = dt.as_secs_f32();

        // Ordered per-frame systems.
        self.apply_input_commands();
        let note_events = self.advance_music(dt);
        self.update_pulses(&note_events, dt_sec);
        self.update_swirl_and_fx(dt_sec);
        self.update_spatial_audio();
        self.update_ambient();
        self.render_scene(dt_sec);
        self.trigger_scheduled_notes(&note_events);
    }

    /// Advance the music engine and return the notes scheduled this frame.
    fn advance_music(&mut self, dt: Duration) -> Vec<NoteEvent> {
        let mut note_events = Vec::new();
        if !*self.paused.borrow() {
            self.engine.borrow_mut().tick(dt, &mut note_events);
        }
        note_events
    }

    /// Accumulate per-voice pulse energy from new notes, then smooth the visual pulses.
    fn update_pulses(&mut self, note_events: &[NoteEvent], dt_sec: f32) {
        let mut pulses_ref = self.pulses.borrow_mut();
        let n = pulses_ref.len().min(3);
        for ev in note_events {
            if ev.voice.0 < n {
                self.pulse_energy[ev.voice.0] =
                    (self.pulse_energy[ev.voice.0] + ev.velocity as f32).min(PULSE_ENERGY_MAX);
            }
        }
        smooth_pulses(&mut pulses_ref, &mut self.pulse_energy, dt_sec);
    }

    /// Update the inertial swirl from pointer input and modulate global FX from it.
    fn update_swirl_and_fx(&mut self, dt_sec: f32) {
        let (uv, mouse_down) = {
            let ms = self.mouse.borrow();
            (input::mouse_uv(&self.canvas, &ms), ms.down)
        };
        self.update_swirl(uv, dt_sec, mouse_down);
        apply_global_fx_swirl(
            &self.reverb_wet,
            &self.delay_wet,
            &self.delay_feedback,
            &self.sat_pre,
            &self.sat_wet,
            &self.sat_dry,
            self.swirl_energy,
            uv,
        );
    }

    /// Position each voice's panner and set its distance-based reverb/delay sends.
    fn update_spatial_audio(&mut self) {
        let voice_positions: Vec<Vec3> = {
            let eng = self.engine.borrow();
            eng.voices.iter().map(|v| v.position).collect()
        };
        for i in 0..self.voice_panners.len() {
            let pos = voice_positions[i];
            self.voice_panners[i].position_x().set_value(pos.x as f32);
            self.voice_panners[i].position_y().set_value(pos.y as f32);
            self.voice_panners[i].position_z().set_value(pos.z as f32);
            let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
            let mut d_amt = (D_SEND_BASE + D_SEND_SPAN * pos.x.abs().min(1.0)).clamp(0.0, 1.0);
            let mut r_amt = (R_SEND_BASE
                + R_SEND_SPAN * (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0))
            .clamp(0.0, R_SEND_CLAMP_MAX);
            let boost = 1.0 + SEND_BOOST_COEFF * self.swirl_energy;
            d_amt = (d_amt * boost).clamp(0.0, D_SEND_CLAMP_MAX);
            r_amt = (r_amt * boost).clamp(0.0, R_SEND_CLAMP_MAX);
            self.delay_sends[i].gain().set_value(d_amt);
            self.reverb_sends[i].gain().set_value(r_amt);
            let lvl = (LEVEL_BASE + LEVEL_SPAN * (1.0 - (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0)))
                as f32;
            self.voice_gains[i].gain().set_value(lvl);
        }
    }

    /// Feed analyser energy into the visual pulses and the background clear amount.
    fn update_ambient(&mut self) {
        if let Some(a) = &self.analyser {
            let bins = a.frequency_bin_count() as usize;
            {
                let mut buf = self.analyser_buf.borrow_mut();
                if buf.len() != bins {
                    buf.resize(bins, 0.0);
                }
                a.get_float_frequency_data(&mut buf);
            }
            let mut sum = 0.0f32;
            let take = bins.min(ANALYSER_BINS_SAMPLED) as u32;
            for i in 0..take {
                let v = self.analyser_buf.borrow()[i as usize];
                let lin = ((v + ANALYSER_DB_FLOOR) / ANALYSER_DB_FLOOR).clamp(0.0, 1.0);
                sum += lin;
            }
            let avg = sum / take as f32;
            {
                let mut pulses_ref = self.pulses.borrow_mut();
                let n = pulses_ref.len().min(3);
                for i in 0..n {
                    pulses_ref[i] = (pulses_ref[i] + avg * AMBIENT_PULSE_GAIN).min(PULSE_MAX);
                }
            }
            if let Some(g) = &mut self.gpu {
                g.set_ambient_clear(avg * AMBIENT_CLEAR_SCALE);
            }
        }
    }

    /// Sync the audio listener to the camera and render the scene (no-op without WebGPU).
    fn render_scene(&mut self, dt_sec: f32) {
        let cam_eye = Vec3::new(0.0, 0.0, CAMERA_Z);
        let cam_target = Vec3::ZERO;
        update_listener_to_camera(&self.listener, cam_eye, cam_target);

        if let Some(g) = &mut self.gpu {
            g.set_camera(cam_eye, cam_target);
            if let Some(uvr) = self.pending_ripple.take() {
                g.set_ripple(uvr, 1.0);
            }
            let speed_norm = (self.swirl_vel[0] * self.swirl_vel[0]
                + self.swirl_vel[1] * self.swirl_vel[1])
                .sqrt()
                .clamp(0.0, 1.0);
            let strength = SWIRL_RENDER_STRENGTH_BASE
                + SWIRL_RENDER_STRENGTH_ENERGY * self.swirl_energy
                + SWIRL_RENDER_STRENGTH_SPEED * speed_norm;
            g.set_swirl(self.swirl_pos, strength, true);
            let w = self.canvas.width();
            let h = self.canvas.height();
            g.resize_if_needed(w, h);
            let voice_positions: Vec<Vec3> = {
                let eng = self.engine.borrow();
                eng.voices.iter().map(|v| v.position).collect()
            };
            let pulse_energy_snapshot: Vec<f32> = self.pulses.borrow().clone();
            if let Err(e) = g.render(dt_sec, &voice_positions, &pulse_energy_snapshot) {
                log::error!("render error: {:?}", e);
            }
        }
    }

    /// Synthesise the notes scheduled this frame (deferred so the render runs first).
    fn trigger_scheduled_notes(&self, note_events: &[NoteEvent]) {
        if *self.paused.borrow() {
            return;
        }
        for ev in note_events {
            let waveform = self.engine.borrow().configs[ev.voice.0].waveform;
            audio::trigger_one_shot(
                &self.audio_ctx,
                waveform,
                ev.freq,
                ev.velocity,
                ev.duration_sec as f64,
                &self.voice_gains[ev.voice.0],
                &self.delay_sends[ev.voice.0],
                &self.reverb_sends[ev.voice.0],
            );
        }
    }
}

impl<'a> FrameContext<'a> {
    fn update_swirl(&mut self, uv: [f32; 2], dt_sec: f32, mouse_down: bool) {
        step_inertial_swirl(
            &mut self.swirl_initialized,
            &mut self.swirl_pos,
            &mut self.swirl_vel,
            uv,
            dt_sec,
        );
        let du = uv[0] - self.prev_uv[0];
        let dv = uv[1] - self.prev_uv[1];
        let pointer_speed = ((du * du + dv * dv).sqrt() / (dt_sec + 1e-5)).min(POINTER_SPEED_MAX);
        let swirl_speed =
            (self.swirl_vel[0] * self.swirl_vel[0] + self.swirl_vel[1] * self.swirl_vel[1]).sqrt();
        let target = ((pointer_speed * SWIRL_TARGET_WEIGHT_POINTER)
            + (swirl_speed * SWIRL_TARGET_WEIGHT_VELOCITY)
            + if mouse_down {
                SWIRL_TARGET_CLICK_BONUS
            } else {
                0.0
            })
        .clamp(0.0, 1.0);
        self.swirl_energy = (1.0 - SWIRL_ENERGY_BLEND_ALPHA) * self.swirl_energy
            + SWIRL_ENERGY_BLEND_ALPHA * target;
        self.prev_uv = uv;
    }

    /// Drain the shared input queue and apply each discrete command. This is the
    /// single point where keyboard/pointer intents mutate engine, audio, and UI
    /// state; the event closures only enqueue.
    fn apply_input_commands(&mut self) {
        let cmds: Vec<InputCommand> = self.input_queue.borrow_mut().drain(..).collect();
        if cmds.is_empty() {
            return;
        }
        let mut params_changed = false;
        for cmd in cmds {
            match cmd {
                InputCommand::SetRoot(root) => {
                    self.engine.borrow_mut().params.root = root;
                    params_changed = true;
                }
                InputCommand::SetScale(scale) => {
                    self.engine.borrow_mut().params.scale = scale;
                    params_changed = true;
                }
                InputCommand::PresetPentatonic => {
                    self.engine.borrow_mut().params.scale = C_MAJOR_PENTATONIC;
                    params_changed = true;
                }
                InputCommand::ReseedAll => {
                    let mut eng = self.engine.borrow_mut();
                    let n = eng.voices.len();
                    for i in 0..n {
                        eng.reseed_voice(VoiceIndex(i), None);
                    }
                    drop(eng);
                    log::info!("[keys] reseeded all voices");
                }
                InputCommand::RandomizeRootMode => {
                    self.engine.borrow_mut().randomize_root_and_mode();
                    params_changed = true;
                }
                InputCommand::TogglePause => {
                    let mut p = self.paused.borrow_mut();
                    *p = !*p;
                    log::info!("[keys] paused={}", *p);
                }
                InputCommand::TempoDelta(d) => {
                    let mut eng = self.engine.borrow_mut();
                    let nb = (eng.params.bpm.0 + d).clamp(40.0, 240.0);
                    eng.set_bpm(Bpm(nb));
                    params_changed = true;
                }
                InputCommand::VolumeDelta(d) => {
                    let v = self.master_gain.gain().value();
                    _ = self.master_gain.gain().set_value((v + d).clamp(0.0, 1.0));
                }
                InputCommand::ToggleMute => {
                    let muted = audio::toggle_master_mute(&self.master_gain);
                    log::info!("[keys] master muted={}", muted);
                }
                InputCommand::DetuneDelta(c) => {
                    self.engine.borrow_mut().adjust_detune_cents(c);
                    params_changed = true;
                }
                InputCommand::ResetDetune => {
                    self.engine.borrow_mut().reset_detune();
                    params_changed = true;
                }
                InputCommand::ToggleFullscreen => {
                    if let Some(doc) = web::window().and_then(|w| w.document()) {
                        if doc.fullscreen_element().is_some() {
                            _ = doc.exit_fullscreen();
                        } else {
                            _ = self.canvas.request_fullscreen();
                        }
                    }
                }
                InputCommand::ExitFullscreen => {
                    if let Some(doc) = web::window().and_then(|w| w.document()) {
                        _ = doc.exit_fullscreen();
                    }
                }
                InputCommand::ToggleHelp => {
                    if let Some(doc) = web::window().and_then(|w| w.document()) {
                        overlay::toggle(&doc);
                    }
                }
                InputCommand::VoiceMute(v) => {
                    self.engine.borrow_mut().toggle_mute(v);
                    log::info!("[click] toggle mute voice {}", v.0);
                }
                InputCommand::VoiceSolo(v) => {
                    self.engine.borrow_mut().toggle_solo(v);
                    log::info!("[click] solo voice {}", v.0);
                }
                InputCommand::VoiceReseed(v) => {
                    self.engine.borrow_mut().reseed_voice(v, None);
                    log::info!("[click] reseed voice {}", v.0);
                }
                InputCommand::PlayNote {
                    voice,
                    freq,
                    velocity,
                    duration_sec,
                } => {
                    let waveform = self.engine.borrow().configs[voice.0].waveform;
                    audio::trigger_one_shot(
                        &self.audio_ctx,
                        waveform,
                        freq,
                        velocity,
                        duration_sec,
                        &self.voice_gains[voice.0],
                        &self.delay_sends[voice.0],
                        &self.reverb_sends[voice.0],
                    );
                }
                InputCommand::Ripple(uv) => self.pending_ripple = Some(uv),
            }
        }
        if params_changed {
            if let Some(doc) = web::window().and_then(|w| w.document()) {
                let (detune, bpm, name) = {
                    let eng = self.engine.borrow();
                    (
                        eng.params.detune.0,
                        eng.params.bpm.0,
                        overlay::scale_name(eng.params.scale),
                    )
                };
                overlay::update_hint(&doc, detune, bpm, name);
                overlay::show_hint(&doc);
            }
        }
    }
}

#[inline]
fn smooth_pulses(pulses: &mut [f32], pulse_energy: &mut [f32; 3], dt_sec: f32) {
    let n = pulses.len().min(3);
    let energy_decay = (-dt_sec * PULSE_ENERGY_DECAY_PER_SEC).exp();
    for i in 0..n {
        pulse_energy[i] *= energy_decay;
    }
    let tau_up = PULSE_RISE_TAU_SEC;
    let tau_down = PULSE_FALL_TAU_SEC;
    let alpha_up = 1.0 - (-dt_sec / tau_up).exp();
    let alpha_down = 1.0 - (-dt_sec / tau_down).exp();
    for i in 0..n {
        let target = pulse_energy[i].clamp(0.0, PULSE_MAX);
        let alpha = if target > pulses[i] {
            alpha_up
        } else {
            alpha_down
        };
        pulses[i] += (target - pulses[i]) * alpha;
    }
}

pub async fn init_gpu(canvas: &web::HtmlCanvasElement) -> Option<render::GpuState<'static>> {
    // leak a canvas clone to satisfy 'static lifetime for surface
    let leaked_canvas = Box::leak(Box::new(canvas.clone()));
    match render::GpuState::new(leaked_canvas, CAMERA_Z).await {
        Ok(g) => {
            log::info!("WebGPU initialized successfully");
            Some(g)
        }
        Err(e) => {
            log::error!("WebGPU init error: {:?}", e);

            // Try to show user-friendly message in DOM
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(error_div) = document.get_element_by_id("no-webgpu") {
                        _ = error_div.set_attribute("style", "display: block");
                    }
                }
            }
            None
        }
    }
}

pub fn start_loop(frame_ctx: Rc<RefCell<FrameContext<'static>>>) {
    let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let tick_clone = tick.clone();
    let frame_ctx_tick = frame_ctx.clone();
    *tick.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        frame_ctx_tick.borrow_mut().frame();
        if let Some(w) = web::window() {
            _ = w.request_animation_frame(
                tick_clone
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unchecked_ref(),
            );
        }
    }) as Box<dyn FnMut()>));
    if let Some(w) = web::window() {
        _ = w.request_animation_frame(tick.borrow().as_ref().unwrap().as_ref().unchecked_ref());
    }
}

// --- helpers private to frame ---
fn step_inertial_swirl(
    initialized: &mut bool,
    swirl_pos: &mut [f32; 2],
    swirl_vel: &mut [f32; 2],
    target_uv: [f32; 2],
    dt_sec: f32,
) {
    if !*initialized {
        *swirl_pos = target_uv;
        swirl_vel[0] = 0.0;
        swirl_vel[1] = 0.0;
        *initialized = true;
        return;
    }
    let omega = SWIRL_OMEGA;
    let k = omega * omega;
    let c = 2.0 * omega * SWIRL_DAMPING_RATIO;
    let dx = target_uv[0] - swirl_pos[0];
    let dy = target_uv[1] - swirl_pos[1];
    let ax = k * dx - c * swirl_vel[0];
    let ay = k * dy - c * swirl_vel[1];
    swirl_vel[0] += ax * dt_sec;
    swirl_vel[1] += ay * dt_sec;
    let mut nx = swirl_pos[0] + swirl_vel[0] * dt_sec;
    let mut ny = swirl_pos[1] + swirl_vel[1] * dt_sec;
    let sdx = nx - swirl_pos[0];
    let sdy = ny - swirl_pos[1];
    let step = (sdx * sdx + sdy * sdy).sqrt();
    let max_step = SWIRL_MAX_STEP_PER_SEC * dt_sec;
    if step > max_step {
        let inv = 1.0 / (step + 1e-6);
        nx = swirl_pos[0] + sdx * inv * max_step;
        ny = swirl_pos[1] + sdy * inv * max_step;
    }
    swirl_pos[0] = nx.clamp(0.0, 1.0);
    swirl_pos[1] = ny.clamp(0.0, 1.0);
}

fn apply_global_fx_swirl(
    reverb_wet: &web::GainNode,
    delay_wet: &web::GainNode,
    delay_feedback: &web::GainNode,
    sat_pre: &web::GainNode,
    sat_wet: &web::GainNode,
    sat_dry: &web::GainNode,
    swirl_energy: f32,
    uv: [f32; 2],
) {
    _ = reverb_wet
        .gain()
        .set_value(FX_REVERB_BASE + FX_REVERB_SPAN * swirl_energy);
    let echo = (uv[0] - uv[1]).abs();
    let delay_wet_val =
        (FX_DELAY_WET_BASE + FX_DELAY_WET_SWIRL * swirl_energy + FX_DELAY_WET_ECHO * echo)
            .clamp(0.0, 1.0);
    let delay_fb_val =
        (FX_DELAY_FB_BASE + FX_DELAY_FB_SWIRL * swirl_energy + FX_DELAY_FB_ECHO * echo)
            .clamp(0.0, 0.95);
    _ = delay_wet.gain().set_value(delay_wet_val);
    _ = delay_feedback.gain().set_value(delay_fb_val);
    let fizz = ((uv[0] + uv[1]) * 0.5).clamp(0.0, 1.0);
    let drive = (FX_SAT_DRIVE_MIN
        + (FX_SAT_DRIVE_MAX - FX_SAT_DRIVE_MIN) * ((fizz - 0.25).clamp(0.0, 1.0)))
    .clamp(FX_SAT_DRIVE_MIN, FX_SAT_DRIVE_MAX);
    _ = sat_pre.gain().set_value(drive);
    let wet = (FX_SAT_WET_BASE + FX_SAT_WET_SPAN * fizz).clamp(0.0, 1.0);
    _ = sat_wet.gain().set_value(wet);
    _ = sat_dry.gain().set_value(1.0 - wet);
}

fn update_listener_to_camera(listener: &web::AudioListener, cam_eye: Vec3, cam_target: Vec3) {
    let fwd = (cam_target - cam_eye).normalize();
    listener.set_position(cam_eye.x as f64, cam_eye.y as f64, cam_eye.z as f64);
    _ = listener.set_orientation(fwd.x as f64, fwd.y as f64, fwd.z as f64, 0.0, 1.0, 0.0);
}
