use crate::audio::{self, ActiveNote};
use crate::core::{MusicEngine, VoiceIndex};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys as web;

const INTERVAL_MS: i32 = 25;
const LOOKAHEAD_SEC: f64 = 0.15;
const ANCHOR_OFFSET_SEC: f64 = 0.06;

/// A pending visual pulse: which voice, at what AudioContext time, its velocity, and the
/// note's normalised pitch. The frame loop fires these when their time arrives so visuals
/// match the sound.
pub type PulseQueue = Rc<RefCell<VecDeque<(VoiceIndex, f64, f32, f32)>>>;

/// Lookahead audio scheduler. Runs on a `setInterval` (off the render frame) and
/// schedules eighth-note grid steps ahead on the AudioContext clock, so timing is
/// sample-accurate and independent of the frame rate.
pub struct AudioScheduler {
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
    audio_ctx: web::AudioContext,
    voice_gains: Rc<Vec<web::GainNode>>,
    delay_sends: Rc<Vec<web::GainNode>>,
    reverb_sends: Rc<Vec<web::GainNode>>,
    active_notes: Rc<RefCell<Vec<ActiveNote>>>,
    pending_pulses: PulseQueue,
    next_note_time: f64,
}

impl AudioScheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: Rc<RefCell<MusicEngine>>,
        paused: Rc<RefCell<bool>>,
        audio_ctx: web::AudioContext,
        voice_gains: Rc<Vec<web::GainNode>>,
        delay_sends: Rc<Vec<web::GainNode>>,
        reverb_sends: Rc<Vec<web::GainNode>>,
        active_notes: Rc<RefCell<Vec<ActiveNote>>>,
        pending_pulses: PulseQueue,
    ) -> Self {
        Self {
            engine,
            paused,
            audio_ctx,
            voice_gains,
            delay_sends,
            reverb_sends,
            active_notes,
            pending_pulses,
            next_note_time: 0.0,
        }
    }

    fn tick(&mut self) {
        let now = self.audio_ctx.current_time();

        // While paused, keep the grid anchored just ahead of "now" so resuming does
        // not burst-schedule a backlog of missed beats.
        if *self.paused.borrow() {
            self.next_note_time = now + ANCHOR_OFFSET_SEC;
            return;
        }
        // First run, or we fell behind (e.g. a backgrounded tab throttled the timer):
        // re-anchor instead of catching up with a burst of past notes.
        if self.next_note_time < now {
            self.next_note_time = now + ANCHOR_OFFSET_SEC;
        }

        while self.next_note_time < now + LOOKAHEAD_SEC {
            let mut events = Vec::new();
            let step_sec = {
                let mut eng = self.engine.borrow_mut();
                eng.step(&mut events);
                eng.params.bpm.eighth_step_seconds().max(0.01)
            };
            for ev in &events {
                let waveform = self.engine.borrow().configs[ev.voice.0].waveform;
                audio::spawn_note(
                    &self.audio_ctx,
                    waveform,
                    ev.freq,
                    ev.velocity,
                    ev.duration_sec as f64,
                    self.next_note_time,
                    &self.voice_gains[ev.voice.0],
                    &self.delay_sends[ev.voice.0],
                    &self.reverb_sends[ev.voice.0],
                    &self.active_notes,
                );
                self.pending_pulses.borrow_mut().push_back((
                    ev.voice,
                    self.next_note_time,
                    ev.velocity,
                    crate::core::pitch_norm(ev.freq.0),
                ));
            }
            self.next_note_time += step_sec;
        }
    }
}

/// Start the scheduler on a `setInterval`. The timer keeps running (coarsely) in
/// background tabs, unlike `requestAnimationFrame`.
pub fn start(scheduler: AudioScheduler) {
    let scheduler = Rc::new(RefCell::new(scheduler));
    let closure = Closure::wrap(Box::new(move || {
        scheduler.borrow_mut().tick();
    }) as Box<dyn FnMut()>);
    if let Some(window) = web::window() {
        _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            INTERVAL_MS,
        );
    }
    closure.forget();
}
