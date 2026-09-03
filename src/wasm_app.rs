use crate::core::{
    default_voice_configs, Bpm, Cents, EngineParams, MidiNote, MusicEngine, PieceArrangement,
    TET31_PENTATONIC,
};
use crate::mastering;
use crate::{audio, constants, dom, events, frame, input, overlay, render, scheduler};
use glam::Vec3;
use js_sys::{Object, Reflect, Uint8Array};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys as web;
use web_time::Instant;

const EXPORT_SAMPLE_RATE: u32 = 48_000;
const MAX_RECORD_SEC: f64 = 1_200.0;

#[derive(Clone)]
struct RuntimeHandles {
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
    composition: Rc<RefCell<Option<PieceArrangement>>>,
    composition_time: Rc<RefCell<f64>>,
    fps_ema: Rc<RefCell<f32>>,
}

thread_local! {
    static RUNTIME: RefCell<Option<RuntimeHandles>> = const { RefCell::new(None) };
}

fn show_audio_error(document: &web::Document, reason: &str) {
    if let Some(el) = document.get_element_by_id("audio-error") {
        let existing_style = el.get_attribute("style").unwrap_or_default();
        let updated_style = format!("{existing_style};display:block;");
        _ = el.set_attribute("style", &updated_style);
        el.set_text_content(Some(&format!(
            "Audio initialization failed ({reason}). If permissions were denied or you are in a headless environment, audio will not play."
        )));
    }
}

fn wire_canvas_resize(canvas: &web::HtmlCanvasElement) {
    dom::sync_canvas_backing_size(canvas);
    let canvas_resize = canvas.clone();
    let resize_closure = Closure::wrap(Box::new(move || {
        dom::sync_canvas_backing_size(&canvas_resize);
    }) as Box<dyn FnMut()>);
    if let Some(window) = web::window() {
        _ = window
            .add_event_listener_with_callback("resize", resize_closure.as_ref().unchecked_ref());
    }
    resize_closure.forget();
}

struct InitParts {
    audio_ctx: web::AudioContext,
    listener_for_tick: web::AudioListener,
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
}

async fn build_audio_and_engine() -> anyhow::Result<InitParts> {
    let audio_ctx = web::AudioContext::new().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    _ = audio_ctx.resume();
    let listener = audio_ctx.listener();
    listener.set_position(0.0, 0.0, 1.5);

    let voice_configs = default_voice_configs();
    let engine = Rc::new(RefCell::new(MusicEngine::new(
        voice_configs,
        EngineParams {
            bpm: Bpm(86.0),
            scale: TET31_PENTATONIC,
            root: MidiNote(50.0),
            detune: Cents(0.0),
        },
        42,
    )));
    {
        let e = engine.borrow();
        log::info!(
            "[engine] voices={} pos0=({:.2},{:.2},{:.2}) pos1=({:.2},{:.2},{:.2}) pos2=({:.2},{:.2},{:.2})",
            e.voices.len(),
            e.voices[0].position.x, e.voices[0].position.y, e.voices[0].position.z,
            e.voices[1].position.x, e.voices[1].position.y, e.voices[1].position.z,
            e.voices[2].position.x, e.voices[2].position.y, e.voices[2].position.z
        );
    }
    let paused = Rc::new(RefCell::new(true));
    Ok(InitParts {
        audio_ctx,
        listener_for_tick: listener,
        engine,
        paused,
    })
}

fn wire_overlay_buttons(audio_ctx: &web::AudioContext, paused: &Rc<RefCell<bool>>) {
    if let Some(doc2) = dom::window_document() {
        let dismiss = |id: &str| {
            let paused = paused.clone();
            let audio_ctx = audio_ctx.clone();
            dom::add_click_listener(&doc2, id, move || {
                *paused.borrow_mut() = false;
                _ = audio_ctx.resume();
                if let Some(d2) = dom::window_document() {
                    overlay::hide(&d2);
                }
            });
        };
        dismiss("overlay-ok");
        dismiss("overlay-close");
    }
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();
    log::info!("app-web starting");

    spawn_local(async move {
        if let Err(e) = init().await {
            log::error!("init error: {:?}", e);
        }
    });
    Ok(())
}

async fn init() -> anyhow::Result<()> {
    let window = web::window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| anyhow::anyhow!("no document"))?;

    let canvas_el = document
        .get_element_by_id("app-canvas")
        .ok_or_else(|| anyhow::anyhow!("missing #app-canvas"))?;
    let canvas: web::HtmlCanvasElement = canvas_el
        .dyn_into::<web::HtmlCanvasElement>()
        .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;

    // Maintain canvas internal pixel size to match CSS size * devicePixelRatio
    wire_canvas_resize(&canvas);

    let canvas_for_click = canvas.clone();

    // Start audio graph and scheduling + WebGPU renderer immediately; show overlay until OK/close
    static STARTED: AtomicBool = AtomicBool::new(false);
    {
        if !STARTED.swap(true, Ordering::SeqCst) {
            let canvas_for_click_inner = canvas_for_click.clone();
            spawn_local(async move {
                let InitParts {
                    audio_ctx,
                    listener_for_tick,
                    engine,
                    paused,
                } = match build_audio_and_engine().await {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("audio init error: {:?}", e);
                        show_audio_error(&document, &format!("{:?}", e));
                        return;
                    }
                };

                wire_overlay_buttons(&audio_ctx, &paused);

                // FX buses
                let fx = match audio::build_fx_buses(&audio_ctx) {
                    Ok(f) => f,
                    Err(e) => {
                        log::error!("audio FX graph initialization failed: {e:?}");
                        show_audio_error(&document, "FX graph initialization failed");
                        return;
                    }
                };
                let master_gain = fx.master_gain.clone();
                let sat_pre = fx.sat_pre.clone();
                let sat_wet = fx.sat_wet.clone();
                let sat_dry = fx.sat_dry.clone();
                let reverb_in = fx.reverb_in.clone();
                let reverb_wet = fx.reverb_wet.clone();
                let delay_in = fx.delay_in.clone();
                let delay_feedback = fx.delay_feedback.clone();
                let delay_wet = fx.delay_wet.clone();
                let sculpture = fx.sculpture.clone();

                // Per-voice master gains -> master bus, plus effect sends
                let initial_positions: Vec<Vec3> =
                    engine.borrow().voices.iter().map(|v| v.position).collect();
                let routing = match audio::wire_voices(
                    &audio_ctx,
                    &initial_positions,
                    &master_gain,
                    &delay_in,
                    &reverb_in,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("voice routing initialization failed: {e:?}");
                        show_audio_error(&document, "voice routing initialization failed");
                        return;
                    }
                };
                let delay_sends = Rc::new(routing.delay_sends);
                let reverb_sends = Rc::new(routing.reverb_sends);
                let voice_panners = routing.voice_panners;
                let voice_gains = Rc::new(routing.voice_gains);

                // Initialize WebGPU
                let gpu: Option<render::GpuState> = frame::init_gpu(&canvas_for_click_inner).await;

                // Visual pulses per voice and optional analyser for ambient effects
                let pulses = Rc::new(RefCell::new(vec![0.0_f32; engine.borrow().voices.len()]));
                let (analyser, analyser_buf) = audio::create_analyser(&audio_ctx);
                if let Some(a) = &analyser {
                    _ = master_gain.connect_with_audio_node(a);
                }

                // Shared note pool and the timed visual pulses the scheduler produces.
                let active_notes: Rc<RefCell<Vec<audio::ActiveNote>>> =
                    Rc::new(RefCell::new(Vec::new()));
                let pending_pulses: scheduler::PulseQueue = Rc::new(RefCell::new(VecDeque::new()));
                let composition: Rc<RefCell<Option<PieceArrangement>>> =
                    Rc::new(RefCell::new(None));
                let composition_time = Rc::new(RefCell::new(0.0_f64));
                let fps_ema = Rc::new(RefCell::new(0.0_f32));

                // Audio scheduler: generates and schedules notes ahead on the audio clock,
                // off the render frame (keeps running, coarsely, in background tabs).
                scheduler::start(scheduler::AudioScheduler::new(
                    engine.clone(),
                    paused.clone(),
                    audio_ctx.clone(),
                    voice_gains.clone(),
                    delay_sends.clone(),
                    reverb_sends.clone(),
                    active_notes.clone(),
                    pending_pulses.clone(),
                ));

                // Shared input command queue: keyboard + pointer enqueue, frame drains.
                let input_queue: Rc<RefCell<VecDeque<events::InputCommand>>> =
                    Rc::new(RefCell::new(VecDeque::new()));

                // ---------------- Interaction state ----------------
                let mouse_state = Rc::new(RefCell::new(input::MouseState::default()));
                let hover_index = Rc::new(RefCell::new(None::<usize>));
                let drag_state = Rc::new(RefCell::new(input::DragState::default()));

                // Keyboard controls
                events::wire_global_keydown(input_queue.clone());

                // Pointer handlers (move/down/up)
                events::wire_input_handlers(events::InputWiring {
                    canvas: canvas_for_click_inner.clone(),
                    engine: engine.clone(),
                    mouse_state: mouse_state.clone(),
                    hover_index: hover_index.clone(),
                    drag_state: drag_state.clone(),
                    queue: input_queue.clone(),
                });

                RUNTIME.with(|runtime| {
                    *runtime.borrow_mut() = Some(RuntimeHandles {
                        engine: engine.clone(),
                        paused: paused.clone(),
                        composition: composition.clone(),
                        composition_time: composition_time.clone(),
                        fps_ema: fps_ema.clone(),
                    });
                });

                // Scheduler + renderer loop driven by requestAnimationFrame
                let frame_ctx = Rc::new(RefCell::new(frame::FrameContext {
                    engine: engine.clone(),
                    paused: paused.clone(),
                    input_queue: input_queue.clone(),
                    pulses: pulses.clone(),
                    canvas: canvas_for_click_inner.clone(),
                    mouse: mouse_state.clone(),
                    audio_ctx: audio_ctx.clone(),
                    master_gain: master_gain.clone(),
                    listener: listener_for_tick.clone(),
                    voice_gains: voice_gains.clone(),
                    delay_sends: delay_sends.clone(),
                    reverb_sends: reverb_sends.clone(),
                    voice_panners,
                    reverb_wet: reverb_wet.clone(),
                    delay_wet: delay_wet.clone(),
                    delay_feedback: delay_feedback.clone(),
                    sat_pre: sat_pre.clone(),
                    sat_wet: sat_wet.clone(),
                    sat_dry: sat_dry.clone(),
                    sculpture: sculpture.clone(),
                    analyser: analyser.clone(),
                    analyser_buf: analyser_buf.clone(),
                    gpu,
                    pending_ripple: None,
                    last_instant: Instant::now(),
                    prev_uv: [0.5, 0.5],
                    swirl_energy: 0.0,
                    swirl_pos: [0.5, 0.5],
                    swirl_vel: [0.0, 0.0],
                    swirl_initialized: false,
                    pulse_energy: [0.0, 0.0, 0.0],
                    voice_pitch: [0.0, 0.0, 0.0],
                    config: constants::Config::default(),
                    active_notes: active_notes.clone(),
                    pending_pulses: pending_pulses.clone(),
                    composition: composition.clone(),
                    composition_time: composition_time.clone(),
                    composition_moment: None,
                    composition_ripple_index: 0,
                    fps_ema: fps_ema.clone(),
                }));
                // Start RAF loop
                frame::start_loop(frame_ctx);
            });
        }
    }

    Ok(())
}

#[wasm_bindgen]
pub fn is_ready() -> bool {
    RUNTIME.with(|runtime| runtime.borrow().is_some())
}

#[wasm_bindgen]
pub fn start_arrangement(duration_secs: f32, seed: u32) {
    let duration = (duration_secs as f64).clamp(1.0, MAX_RECORD_SEC);
    RUNTIME.with(|runtime| {
        if let Some(handles) = runtime.borrow().as_ref() {
            let arrangement = PieceArrangement::new(duration, seed);
            *handles.composition.borrow_mut() = Some(arrangement);
            *handles.composition_time.borrow_mut() = 0.0;
            *handles.paused.borrow_mut() = false;
            let moment = arrangement.moment(0.0);
            let mut engine = handles.engine.borrow_mut();
            engine.reseed_all_from(seed as u64);
            moment.apply_to_engine(&mut engine);
        }
    });
}

#[wasm_bindgen]
pub fn stop_arrangement() {
    RUNTIME.with(|runtime| {
        if let Some(handles) = runtime.borrow().as_ref() {
            *handles.composition.borrow_mut() = None;
        }
    });
}

#[wasm_bindgen]
pub fn arrangement_active() -> bool {
    RUNTIME.with(|runtime| {
        runtime
            .borrow()
            .as_ref()
            .is_some_and(|handles| handles.composition.borrow().is_some())
    })
}

#[wasm_bindgen]
pub fn fps() -> f32 {
    RUNTIME.with(|runtime| {
        runtime
            .borrow()
            .as_ref()
            .map(|handles| *handles.fps_ema.borrow())
            .unwrap_or(0.0)
    })
}

#[wasm_bindgen]
pub async fn generate_piece(
    duration_secs: f32,
    seed: u32,
    target_lufs: f32,
) -> Result<JsValue, JsValue> {
    let duration = (duration_secs as f64).clamp(1.0, MAX_RECORD_SEC);
    let arrangement = PieceArrangement::new(duration, seed);
    let (left, right) =
        audio::render_piece_offline(&arrangement, EXPORT_SAMPLE_RATE, 0.30, seed as u64)
            .await
            .ok_or_else(|| JsValue::from_str("Offline audio render failed in this browser."))?;
    let settings = mastering::MasterSettings {
        sample_rate: EXPORT_SAMPLE_RATE,
        target_lufs,
        true_peak_ceiling_db: -1.0,
    };
    let (ml, mr, report) = mastering::master(&left, &right, &settings);
    let wav = mastering::encode_wav_24(&ml, &mr, EXPORT_SAMPLE_RATE);
    build_export_result(&wav, &report, EXPORT_SAMPLE_RATE)
}

fn build_export_result(
    wav: &[u8],
    report: &mastering::MasterReport,
    sample_rate: u32,
) -> Result<JsValue, JsValue> {
    let obj = Object::new();
    let wav_array = Uint8Array::new_with_length(wav.len() as u32);
    wav_array.copy_from(wav);
    Reflect::set(&obj, &"wav".into(), &wav_array.into())?;
    Reflect::set(
        &obj,
        &"report".into(),
        &format_report(report, sample_rate).into(),
    )?;
    Reflect::set(&obj, &"lufs".into(), &report.lufs_out.into())?;
    Reflect::set(&obj, &"truePeakDb".into(), &report.true_peak_db.into())?;
    Reflect::set(&obj, &"durationSec".into(), &report.duration_secs.into())?;
    Reflect::set(&obj, &"sampleRate".into(), &sample_rate.into())?;
    Ok(obj.into())
}

fn format_report(r: &mastering::MasterReport, sr: u32) -> String {
    format!(
        "{:.0}s - {} kHz / 24-bit WAV\nLoudness: {:.1} -> {:.1} LUFS ({:+.1} dB)\nTrue peak: {:.1} dBTP - Sample peak: {:.1} dBFS\nStereo: {:.2} correlation - Tonal tilt: {:+.1} dB\n{}",
        r.duration_secs,
        sr / 1000,
        r.lufs_in,
        r.lufs_out,
        r.gain_db,
        r.true_peak_db,
        r.sample_peak_db,
        r.stereo_correlation,
        r.spectral_tilt_db,
        if r.limited {
            "Peak limiter engaged to hold the -1 dBTP ceiling."
        } else {
            "Clean headroom - no limiting needed."
        }
    )
}
