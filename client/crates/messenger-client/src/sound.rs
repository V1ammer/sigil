//! Notification sound — a short generated "ding" played on incoming messages.
//!
//! No asset needed: a 16-bit PCM WAV is synthesized once, turned into a Blob
//! object URL, and replayed through a reused `<audio>` element.

use std::cell::RefCell;

/// A settings bool from localStorage, defaulting to `true` (on).
fn setting_on(key: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(key).ok().flatten())
        .map_or(true, |v| v != "false")
}

thread_local! {
    static AUDIO: RefCell<Option<web_sys::HtmlAudioElement>> = const { RefCell::new(None) };
    static UNLOCKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Play the new-message sound, respecting the user's notification settings.
///
/// Silently no-ops if notifications or the sound are disabled, or if the
/// browser blocks playback (e.g. before the first user gesture).
pub fn play_message_sound() {
    if !setting_on("ms_settings_notifications_enabled")
        || !setting_on("ms_settings_notification_sound")
    {
        return;
    }

    AUDIO.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            if let Some(url) = build_beep_object_url() {
                if let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(&url) {
                    audio.set_volume(0.4);
                    *slot = Some(audio);
                }
            }
        }
        if let Some(audio) = slot.as_ref() {
            audio.set_current_time(0.0);
            // play() returns a Promise; a rejection (no user gesture yet) is fine.
            let _ = audio.play();
        }
    });
}

/// Prime the audio element during a real user gesture so later notification
/// sounds aren't blocked by the browser autoplay policy. Registers a one-time
/// `pointerdown`/`keydown` listener; safe to call once at startup.
pub fn arm_audio_unlock() {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };

    let cb = Closure::<dyn FnMut()>::new(move || {
        if UNLOCKED.with(std::cell::Cell::get) {
            return;
        }
        AUDIO.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                if let Some(url) = build_beep_object_url() {
                    if let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(&url) {
                        audio.set_volume(0.4);
                        *slot = Some(audio);
                    }
                }
            }
            // A muted play during the gesture unlocks the element for later.
            if let Some(audio) = slot.as_ref() {
                audio.set_muted(true);
                let _ = audio.play();
                audio.set_current_time(0.0);
                audio.set_muted(false);
            }
        });
        UNLOCKED.with(|u| u.set(true));
    });
    let f = cb.as_ref().unchecked_ref::<js_sys::Function>();
    let _ = doc.add_event_listener_with_callback("pointerdown", f);
    let _ = doc.add_event_listener_with_callback("keydown", f);
    cb.forget();
}

fn build_beep_object_url() -> Option<String> {
    let wav = generate_beep_wav();
    let u8a = js_sys::Uint8Array::from(wav.as_slice());
    let parts = js_sys::Array::new();
    parts.push(&u8a.into());
    let mut bag = web_sys::BlobPropertyBag::new();
    bag.type_("audio/wav");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &bag).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

/// Synthesize a short, soft two-note "ding" as a 16-bit mono PCM WAV.
fn generate_beep_wav() -> Vec<u8> {
    const SAMPLE_RATE: u32 = 44_100;
    let duration = 0.20_f32;
    let n = (SAMPLE_RATE as f32 * duration) as usize;

    let mut samples: Vec<i16> = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SAMPLE_RATE as f32;
        let p = i as f32 / n as f32; // 0..1 progress
        // Rising two-note feel: 660 Hz then 880 Hz.
        let freq = if p < 0.5 { 660.0 } else { 880.0 };
        // Attack over the first 6%, linear decay to silence after — no clicks.
        let env = (p / 0.06).min(1.0) * (1.0 - p);
        let v = (2.0 * std::f32::consts::PI * freq * t).sin() * env * 0.6;
        samples.push((v * f32::from(i16::MAX)) as i16);
    }

    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate (mono, 16-bit)
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}
