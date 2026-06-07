use crate::events::command::{command_for_key, key_prevents_default, InputCommand};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys as web;

/// Wire a single global `keydown` listener that maps keys to `InputCommand`s and
/// enqueues them. All keyboard intents — including the help-panel toggle (`H`) —
/// flow through this one handler and the shared input queue; the frame loop
/// applies them.
pub fn wire_global_keydown(queue: Rc<RefCell<VecDeque<InputCommand>>>) {
    if let Some(window) = web::window() {
        let closure =
            wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
                let key = ev.key();
                if let Some(cmd) = command_for_key(&key, ev.shift_key()) {
                    queue.borrow_mut().push_back(cmd);
                    if key_prevents_default(&key) {
                        ev.prevent_default();
                    }
                }
            }) as Box<dyn FnMut(_)>);
        _ = window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}
