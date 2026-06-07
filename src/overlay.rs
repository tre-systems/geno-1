use crate::core::{
    AEOLIAN, C_MAJOR_PENTATONIC, DORIAN, IONIAN, LOCRIAN, LYDIAN, MIXOLYDIAN, PHRYGIAN,
    TET19_PENTATONIC, TET24_PENTATONIC, TET31_PENTATONIC,
};
use web_sys as web;

/// Human-readable name for a scale slice, for the hint overlay.
pub fn scale_name(scale: &[f32]) -> &'static str {
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

#[inline]
pub fn show(document: &web::Document) {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        let cl = el.class_list();
        _ = cl.remove_1("hidden");
        // fallback for environments without CSS class
        _ = el.set_attribute("style", "");
    }
}

#[inline]
pub fn hide(document: &web::Document) {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        let cl = el.class_list();
        _ = cl.add_1("hidden");
        // fallback
        _ = el.set_attribute("style", "display:none");
    }
}

#[inline]
pub fn is_hidden(document: &web::Document) -> bool {
    if let Some(el) = document.get_element_by_id("start-overlay") {
        if el.class_list().contains("hidden") {
            return true;
        }
        return el
            .get_attribute("style")
            .map(|s| s.contains("display:none"))
            .unwrap_or(false);
    }
    false
}

#[inline]
pub fn toggle(document: &web::Document) {
    if is_hidden(document) {
        show(document);
    } else {
        hide(document);
    }
}

/// Update the hint overlay with current engine state
pub fn update_hint(document: &web::Document, detune_cents: f32, bpm: f32, scale_name: &str) {
    if let Some(el) = document.get_element_by_id("hint-overlay") {
        let detune_text = if detune_cents.abs() < 0.1 {
            "Detune: 0¢".to_string()
        } else {
            let sign = if detune_cents > 0.0 { "+" } else { "" };
            format!("Detune: {}{:.0}¢", sign, detune_cents)
        };

        let bpm_text = format!("BPM: {:.0}", bpm);
        let scale_text = format!("Scale: {}", scale_name);

        let hint_html = format!(
            "<div style='color: #cfe7ff; font: 13px system-ui; background: rgba(10, 14, 24, 0.8); padding: 8px 12px; border-radius: 6px; border: 1px solid rgba(80, 110, 150, 0.35);'>{} • {} • {}</div>",
            detune_text, bpm_text, scale_text
        );

        el.set_inner_html(&hint_html);
    }
}

/// Show the hint overlay
pub fn show_hint(document: &web::Document) {
    if let Some(el) = document.get_element_by_id("hint-overlay") {
        el.set_attribute("style", "").ok();
    }
}
