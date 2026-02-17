use crate::core::MusicEngine;
use crate::core::{
    AEOLIAN, C_MAJOR_PENTATONIC, DORIAN, IONIAN, LOCRIAN, LYDIAN, MIXOLYDIAN, PHRYGIAN,
    TET19_PENTATONIC, TET24_PENTATONIC, TET31_PENTATONIC,
};
use crate::events::keymap::{mode_scale_for_digit, root_midi_for_key};
use crate::overlay;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

thread_local! {
    static MASTER_UNMUTED_GAIN: RefCell<Option<f32>> = RefCell::new(None);
}

/// Get the name of the current scale for display purposes
fn get_scale_name(scale: &[f32]) -> &'static str {
    match scale {
        s if s == IONIAN => "Ionian (major)",
        s if s == DORIAN => "Dorian",
        s if s == PHRYGIAN => "Phrygian",
        s if s == LYDIAN => "Lydian",
        s if s == MIXOLYDIAN => "Mixolydian",
        s if s == AEOLIAN => "Aeolian (minor)",
        s if s == LOCRIAN => "Locrian",
        s if s == C_MAJOR_PENTATONIC => "C Major Pentatonic",
        s if s == TET19_PENTATONIC => "19-TET pentatonic",
        s if s == TET24_PENTATONIC => "24-TET pentatonic",
        s if s == TET31_PENTATONIC => "31-TET pentatonic",
        _ => "Custom",
    }
}

/// Update the hint overlay after engine parameter changes
fn update_hint_after_change(engine: &Rc<RefCell<MusicEngine>>) {
    if let Some(window) = web::window() {
        if let Some(document) = window.document() {
            let (detune, bpm, scale_name) = {
                let eng = engine.borrow();
                (
                    eng.params.detune_cents,
                    eng.params.bpm,
                    get_scale_name(eng.params.scale),
                )
            };
            overlay::update_hint(&document, detune, bpm, scale_name);
            overlay::show_hint(&document);
        }
    }
}

pub fn handle_global_keydown(
    ev: &web::KeyboardEvent,
    engine: &Rc<RefCell<MusicEngine>>,
    paused: &Rc<RefCell<bool>>,
    master_gain: &web::GainNode,
    canvas: &web::HtmlCanvasElement,
) {
    let key = ev.key();
    if let Some(midi) = root_midi_for_key(&key) {
        engine.borrow_mut().params.root_midi = midi;
        update_hint_after_change(engine);
        return;
    }
    if let Some(scale) = mode_scale_for_digit(&key) {
        engine.borrow_mut().params.scale = scale;
        update_hint_after_change(engine);
        return;
    }
    match key.as_str() {
        "p" | "P" => {
            engine.borrow_mut().params.scale = C_MAJOR_PENTATONIC;
            update_hint_after_change(engine);
            return;
        }
        "r" | "R" => {
            let voice_len = engine.borrow().voices.len();
            let mut eng = engine.borrow_mut();
            for i in 0..voice_len {
                eng.reseed_voice(i, None);
            }
            log::info!("[keys] reseeded all voices");
        }
        "t" | "T" => {
            let roots: [i32; 7] = [60, 62, 64, 65, 67, 69, 71]; // C, D, E, F, G, A, B
            let modes: [&'static [f32]; 7] = [
                IONIAN, DORIAN, PHRYGIAN, LYDIAN, MIXOLYDIAN, AEOLIAN, LOCRIAN,
            ];
            let ri = (js_sys::Math::random() * roots.len() as f64).floor() as usize;
            let mi = (js_sys::Math::random() * modes.len() as f64).floor() as usize;
            let mut eng = engine.borrow_mut();
            eng.params.root_midi = roots[ri];
            eng.params.scale = modes[mi];
            drop(eng);
            update_hint_after_change(engine);
        }
        " " => {
            let mut p = paused.borrow_mut();
            *p = !*p;
            log::info!("[keys] paused={}", *p);
            ev.prevent_default();
        }
        "ArrowRight" | "+" | "=" => {
            let mut eng = engine.borrow_mut();
            let new_bpm = (eng.params.bpm + 5.0).min(240.0);
            eng.set_bpm(new_bpm);
            drop(eng);
            update_hint_after_change(engine);
        }
        "ArrowLeft" | "-" | "_" => {
            let mut eng = engine.borrow_mut();
            let new_bpm = (eng.params.bpm - 5.0).max(40.0);
            eng.set_bpm(new_bpm);
            drop(eng);
            update_hint_after_change(engine);
        }
        "m" | "M" => {
            let current_gain = master_gain.gain().value();
            if current_gain <= 0.0001 {
                let restored = MASTER_UNMUTED_GAIN
                    .with(|state| state.borrow_mut().take())
                    .unwrap_or(0.25)
                    .clamp(0.0, 1.0);
                _ = master_gain.gain().set_value(restored);
                log::info!("[keys] master muted=false");
            } else {
                MASTER_UNMUTED_GAIN.with(|state| *state.borrow_mut() = Some(current_gain));
                _ = master_gain.gain().set_value(0.0);
                log::info!("[keys] master muted=true");
            }
            ev.prevent_default();
        }
        "," => {
            let mut eng = engine.borrow_mut();
            if ev.shift_key() {
                eng.adjust_detune_cents(-10.0); // Fine adjustment
            } else {
                eng.adjust_detune_cents(-50.0); // Coarse adjustment
            }
            drop(eng);
            update_hint_after_change(engine);
        }
        "." => {
            let mut eng = engine.borrow_mut();
            if ev.shift_key() {
                eng.adjust_detune_cents(10.0); // Fine adjustment
            } else {
                eng.adjust_detune_cents(50.0); // Coarse adjustment
            }
            drop(eng);
            update_hint_after_change(engine);
        }
        "/" => {
            let mut eng = engine.borrow_mut();
            eng.reset_detune();
            drop(eng);
            update_hint_after_change(engine);
        }
        "Enter" => {
            if let Some(win) = web::window() {
                if let Some(doc) = win.document() {
                    if doc.fullscreen_element().is_some() {
                        _ = doc.exit_fullscreen();
                    } else {
                        _ = canvas.request_fullscreen();
                    }
                }
            }
            ev.prevent_default();
        }
        "Escape" => {
            if let Some(win) = web::window() {
                if let Some(doc) = win.document() {
                    _ = doc.exit_fullscreen();
                }
            }
        }
        _ => {}
    }
    match key.as_str() {
        "ArrowUp" => {
            let v = master_gain.gain().value();
            let nv = (v + 0.05).min(1.0);
            _ = master_gain.gain().set_value(nv);
            ev.prevent_default();
        }
        "ArrowDown" => {
            let v = master_gain.gain().value();
            let nv = (v - 0.05).max(0.0);
            _ = master_gain.gain().set_value(nv);
            ev.prevent_default();
        }
        _ => {}
    }
}

// Wire an 'H' key handler to toggle the overlay without affecting pause state
pub fn wire_overlay_toggle_h(document: &web::Document) {
    if let Some(window) = web::window() {
        let doc = document.clone();
        let closure =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
                let key = ev.key();
                if key == "h" || key == "H" {
                    crate::overlay::toggle(&doc);
                    ev.prevent_default();
                }
            }) as Box<dyn FnMut(_)>);
        _ = window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

pub fn wire_global_keydown(
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
    master_gain: web::GainNode,
    canvas: web::HtmlCanvasElement,
) {
    if let Some(window) = web::window() {
        let closure =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
                super::keyboard::handle_global_keydown(
                    &ev,
                    &engine,
                    &paused,
                    &master_gain,
                    &canvas,
                );
            }) as Box<dyn FnMut(_)>);
        _ = window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}
