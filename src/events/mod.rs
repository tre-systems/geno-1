pub mod command;
#[cfg(target_arch = "wasm32")]
pub mod keyboard;
pub mod keymap;
#[cfg(target_arch = "wasm32")]
pub mod pointer;

pub use command::InputCommand;
#[cfg(target_arch = "wasm32")]
pub use keyboard::wire_global_keydown;
#[cfg(target_arch = "wasm32")]
pub use pointer::{wire_input_handlers, InputWiring};
