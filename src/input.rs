#[cfg(target_arch = "wasm32")]
use glam::Vec2;
use glam::Vec3;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys as web;

#[derive(Default, Clone, Copy)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub down: bool,
}
#[derive(Default, Clone, Copy)]
pub struct DragState {
    pub active: bool,
    pub voice: usize,
    pub plane_z_world: f32,
}
#[inline]
pub fn ray_sphere(ray_origin: Vec3, ray_dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = ray_origin - center;
    let b = oc.dot(ray_dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let sqrt_disc = disc.sqrt();
    let t_near = -b - sqrt_disc;
    if t_near >= 0.0 {
        return Some(t_near);
    }
    let t_far = -b + sqrt_disc;
    (t_far >= 0.0).then_some(t_far)
}

#[inline]
#[cfg(target_arch = "wasm32")]
pub fn pointer_canvas_px(ev: &web::PointerEvent, canvas: &web::HtmlCanvasElement) -> Vec2 {
    let el: web::Element = canvas.clone().unchecked_into();
    let rect = el.get_bounding_client_rect();
    let x_css = ev.client_x() as f32 - rect.left() as f32;
    let y_css = ev.client_y() as f32 - rect.top() as f32;
    let sx = (x_css / rect.width() as f32) * canvas.width() as f32;
    let sy = (y_css / rect.height() as f32) * canvas.height() as f32;
    Vec2::new(sx, sy)
}

#[inline]
#[cfg(target_arch = "wasm32")]
pub fn pointer_canvas_uv(ev: &web::PointerEvent, canvas: &web::HtmlCanvasElement) -> [f32; 2] {
    let el: web::Element = canvas.clone().unchecked_into();
    let rect = el.get_bounding_client_rect();
    let x_css = ev.client_x() as f32 - rect.left() as f32;
    let y_css = ev.client_y() as f32 - rect.top() as f32;
    let w = rect.width() as f32;
    let h = rect.height() as f32;
    if w > 0.0 && h > 0.0 {
        let u = (x_css / w).clamp(0.0, 1.0);
        let v = (y_css / h).clamp(0.0, 1.0);
        [u, v]
    } else {
        [0.5, 0.5]
    }
}

#[inline]
#[cfg(target_arch = "wasm32")]
pub fn mouse_uv(canvas: &web::HtmlCanvasElement, mouse: &MouseState) -> [f32; 2] {
    let w = canvas.width().max(1) as f32;
    let h = canvas.height().max(1) as f32;
    [(mouse.x / w).clamp(0.0, 1.0), (mouse.y / h).clamp(0.0, 1.0)]
}

// ---------------- Selection helpers ----------------
#[inline]
pub fn nearest_index_by_uvx(normalized_voice_xs: &[f32], uvx: f32) -> usize {
    let mut best_i = 0usize;
    let mut best_dx = f32::MAX;
    for (i, vx) in normalized_voice_xs.iter().enumerate() {
        let dx = (uvx - *vx).abs();
        if dx < best_dx {
            best_dx = dx;
            best_i = i;
        }
    }
    best_i
}
