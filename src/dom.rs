use wasm_bindgen::JsCast;
use web_sys as web;

#[inline]
pub fn window_document() -> Option<web::Document> {
    web::window().and_then(|w| w.document())
}

#[inline]
pub fn add_click_listener(
    document: &web::Document,
    element_id: &str,
    handler: impl FnMut() + 'static,
) {
    if let Some(el) = document.get_element_by_id(element_id) {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(handler) as Box<dyn FnMut()>);
        _ = el.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

pub fn sync_canvas_backing_size(canvas: &web::HtmlCanvasElement) {
    if let Some(w) = web::window() {
        let dpr = w.device_pixel_ratio();
        let el: web::Element = canvas.clone().unchecked_into();
        let rect = el.get_bounding_client_rect();
        let w_px = (rect.width() * dpr) as u32;
        let h_px = (rect.height() * dpr) as u32;
        canvas.set_width(w_px.max(1));
        canvas.set_height(h_px.max(1));
    }
}
