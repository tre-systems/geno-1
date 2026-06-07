use crate::core::{
    Bpm, Cents, EngineParams, MidiNote, MusicEngine, VoiceConfig, Waveform, C_MAJOR_PENTATONIC,
};
use crate::{audio, constants, dom, events, frame, input, overlay, render};
use glam::Vec3;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys as web;
use web_time::Instant;

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

async fn build_audio_and_engine(_document: web::Document) -> anyhow::Result<InitParts> {
    let audio_ctx = web::AudioContext::new().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    _ = audio_ctx.resume();
    let listener = audio_ctx.listener();
    listener.set_position(0.0, 0.0, 1.5);

    let voice_configs = vec![
        VoiceConfig {
            waveform: Waveform::Sine,
            base_position: Vec3::new(-1.0, 0.0, 0.0),
            trigger_probability: 0.4,
            octave_offset: -1,
            base_duration: 0.4,
        },
        VoiceConfig {
            waveform: Waveform::Saw,
            base_position: Vec3::new(1.0, 0.0, 0.0),
            trigger_probability: 0.6,
            octave_offset: 0,
            base_duration: 0.25,
        },
        VoiceConfig {
            waveform: Waveform::Triangle,
            base_position: Vec3::new(0.0, 0.0, -1.0),
            trigger_probability: 0.3,
            octave_offset: 1,
            base_duration: 0.6,
        },
    ];
    let engine = Rc::new(RefCell::new(MusicEngine::new(
        voice_configs,
        EngineParams {
            bpm: Bpm(110.0),
            scale: C_MAJOR_PENTATONIC,
            root: MidiNote(60.0),
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
        let paused_ok = paused.clone();
        let audio_ok = audio_ctx.clone();
        dom::add_click_listener(&doc2, "overlay-ok", move || {
            *paused_ok.borrow_mut() = false;
            _ = audio_ok.resume();
            if let Some(w2) = web::window() {
                if let Some(d2) = w2.document() {
                    overlay::hide(&d2);
                }
            }
        });

        let paused_close = paused.clone();
        let audio_close = audio_ctx.clone();
        dom::add_click_listener(&doc2, "overlay-close", move || {
            *paused_close.borrow_mut() = false;
            _ = audio_close.resume();
            if let Some(w2) = web::window() {
                if let Some(d2) = w2.document() {
                    overlay::hide(&d2);
                }
            }
        });
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

    // Note: start overlay is handled below (toggle with 'h') once audio is initialized.

    // Avoid grabbing a 2D context here to allow WebGPU to acquire the canvas

    // Maintain canvas internal pixel size to match CSS size * devicePixelRatio
    wire_canvas_resize(&canvas);

    // Prepare a clone for use inside the click closure
    let canvas_for_click = canvas.clone();

    // Start audio graph and scheduling + WebGPU renderer immediately; show overlay until OK/close
    static STARTED: AtomicBool = AtomicBool::new(false);
    {
        if STARTED.swap(true, Ordering::SeqCst) == false {
            let canvas_for_click_inner = canvas_for_click.clone();
            spawn_local(async move {
                let InitParts {
                    audio_ctx,
                    listener_for_tick,
                    engine,
                    paused,
                } = match build_audio_and_engine(document.clone()).await {
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

                // Scheduler + renderer loop driven by requestAnimationFrame
                let frame_ctx = Rc::new(RefCell::new(frame::FrameContext {
                    engine: engine.clone(),
                    paused: paused.clone(),
                    input_queue: input_queue.clone(),
                    pulses: pulses.clone(),
                    hover_index: hover_index.clone(),
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
                    config: constants::Config::default(),
                    active_notes: Vec::new(),
                }));
                // Start RAF loop
                frame::start_loop(frame_ctx);
            });
        }
    }

    Ok(())
}
