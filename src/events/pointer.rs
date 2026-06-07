use crate::audio;
use crate::constants::{
    CAMERA_Z, CLICK_DUR_BASE_SEC, CLICK_DUR_SPAN_SEC, CLICK_NOTE_BASE_MIDI, CLICK_NOTE_MIDI_SPAN,
    CLICK_VEL_BASE, CLICK_VEL_SPAN, ENGINE_DRAG_MAX_RADIUS, PICK_SPHERE_RADIUS, SPREAD, Z_OFFSET,
};
use crate::core::{MidiNote, MusicEngine, VoiceIndex};
use crate::input;
use crate::render;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

#[derive(Clone)]
pub struct InputWiring {
    pub canvas: web::HtmlCanvasElement,
    pub engine: Rc<RefCell<MusicEngine>>,
    pub mouse_state: Rc<RefCell<input::MouseState>>,
    pub hover_index: Rc<RefCell<Option<usize>>>,
    pub drag_state: Rc<RefCell<input::DragState>>,
    pub voice_gains: Rc<Vec<web::GainNode>>,
    pub delay_sends: Rc<Vec<web::GainNode>>,
    pub reverb_sends: Rc<Vec<web::GainNode>>,
    pub audio_ctx: web::AudioContext,
    pub queued_ripple_uv: Rc<RefCell<Option<[f32; 2]>>>,
}

pub fn wire_input_handlers(w: InputWiring) {
    wire_pointermove(&w);
    wire_pointerdown(&w);
    wire_pointerup(&w);
}

fn wire_pointermove(w: &InputWiring) {
    let w = w.clone();
    let canvas_connected = w.canvas.is_connected();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let pos = input::pointer_canvas_px(&ev, &w.canvas);

        if !canvas_connected {
            return;
        }

        {
            let mut ms = w.mouse_state.borrow_mut();
            ms.x = pos.x;
            ms.y = pos.y;
        }

        let (ro, rd) = render::screen_to_world_ray(&w.canvas, pos.x, pos.y, CAMERA_Z);
        let mut best = None::<(usize, f32)>;
        let z_offset = Z_OFFSET;

        let engine_snapshot = w.engine.borrow();
        for (i, v) in engine_snapshot.voices.iter().enumerate() {
            let center_world = v.position * SPREAD + z_offset;

            if let Some(t) = input::ray_sphere(ro, rd, center_world, PICK_SPHERE_RADIUS) {
                if t >= 0.0 {
                    match best {
                        Some((_, bt)) if t >= bt => {}
                        _ => best = Some((i, t)),
                    }
                }
            }
        }
        if w.drag_state.borrow().active {
            let plane_z = w.drag_state.borrow().plane_z_world;

            if rd.z.abs() > 1e-6 {
                let t = (plane_z - ro.z) / rd.z;

                if t >= 0.0 {
                    let hit_world = ro + rd * t;
                    let mut eng_pos = (hit_world - Z_OFFSET) / SPREAD;
                    let max_r = ENGINE_DRAG_MAX_RADIUS;
                    let len = (eng_pos.x * eng_pos.x + eng_pos.z * eng_pos.z).sqrt();

                    if len > max_r {
                        let scale = max_r / len;
                        eng_pos.x *= scale;
                        eng_pos.z *= scale;
                    }

                    let vi = w.drag_state.borrow().voice;
                    let mut eng = w.engine.borrow_mut();
                    eng.set_voice_position(
                        VoiceIndex(vi),
                        glam::Vec3::new(eng_pos.x, 0.0, eng_pos.z),
                    );
                }
            }
        } else {
            match best {
                Some((i, _t)) => {
                    *w.hover_index.borrow_mut() = Some(i);
                }
                None => {
                    *w.hover_index.borrow_mut() = None;
                }
            }
        }
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointermove", closure.as_ref().unchecked_ref());
    }

    closure.forget();
}

fn wire_pointerdown(w: &InputWiring) {
    let w = w.clone();
    let canvas_for_listener = w.canvas.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        if let Some(i) = *w.hover_index.borrow() {
            let mut ds = w.drag_state.borrow_mut();
            ds.active = true;
            ds.voice = i;
            ds.plane_z_world = w.engine.borrow().voices[i].position.z * SPREAD.z + Z_OFFSET.z;
            log::info!("[mouse] begin drag on voice {}", i);
        }
        w.mouse_state.borrow_mut().down = true;
        _ = w.canvas.set_pointer_capture(ev.pointer_id());
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);
    _ = canvas_for_listener
        .add_event_listener_with_callback("pointerdown", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn wire_pointerup(w: &InputWiring) {
    let w = w.clone();

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::PointerEvent| {
        let was_dragging = w.drag_state.borrow().active;

        if was_dragging {
            w.drag_state.borrow_mut().active = false;
        } else if let Some(i) = *w.hover_index.borrow() {
            let shift = ev.shift_key();
            let alt = ev.alt_key();
            if alt {
                w.engine.borrow_mut().toggle_solo(VoiceIndex(i));
                log::info!("[click] solo voice {}", i);
            } else if shift {
                w.engine.borrow_mut().reseed_voice(VoiceIndex(i), None);
                log::info!("[click] reseed voice {}", i);
            } else {
                w.engine.borrow_mut().toggle_mute(VoiceIndex(i));
                log::info!("[click] toggle mute voice {}", i);
            }
        } else {
            let [uvx, uvy] = input::pointer_canvas_uv(&ev, &w.canvas);
            if uvx.is_finite() && uvy.is_finite() {
                let freq = MidiNote(CLICK_NOTE_BASE_MIDI + uvx * CLICK_NOTE_MIDI_SPAN).to_hz();
                let vel = CLICK_VEL_BASE + CLICK_VEL_SPAN * uvy;
                let eng = w.engine.borrow();
                let norm_xs: Vec<f32> = eng
                    .voices
                    .iter()
                    .map(|v| (v.position.x / 3.0).clamp(-1.0, 1.0) * 0.5 + 0.5)
                    .collect();
                let best_i = crate::input::nearest_index_by_uvx(&norm_xs, uvx);
                let dur = CLICK_DUR_BASE_SEC + CLICK_DUR_SPAN_SEC * (1.0 - uvy as f64);
                let wf = eng.configs[best_i].waveform;
                drop(eng);
                audio::trigger_one_shot(
                    &w.audio_ctx,
                    wf,
                    freq,
                    vel,
                    dur,
                    &w.voice_gains[best_i],
                    &w.delay_sends[best_i],
                    &w.reverb_sends[best_i],
                );
                *w.queued_ripple_uv.borrow_mut() = Some([uvx, uvy]);
            }
        }
        w.mouse_state.borrow_mut().down = false;
        ev.prevent_default();
    }) as Box<dyn FnMut(_)>);

    if let Some(wnd) = web::window() {
        _ = wnd.add_event_listener_with_callback("pointerup", closure.as_ref().unchecked_ref());
    }

    closure.forget();
}
