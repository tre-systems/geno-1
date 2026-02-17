#[cfg(target_arch = "wasm32")]
pub mod keyboard;
pub mod keymap;
#[cfg(target_arch = "wasm32")]
pub mod pointer;

#[cfg(target_arch = "wasm32")]
pub use keyboard::{wire_global_keydown, wire_overlay_toggle_h};
#[cfg(target_arch = "wasm32")]
pub use pointer::{wire_input_handlers, InputWiring};
