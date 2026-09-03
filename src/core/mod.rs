pub mod music;
pub mod piece;
pub mod units;

pub use music::*;
pub use piece::*;
pub use units::*;

// Shaders bundled as string constants
pub static POST_WGSL: &str = include_str!("../../shaders/post.wgsl");
pub static WAVES_WGSL: &str = include_str!("../../shaders/waves.wgsl");
