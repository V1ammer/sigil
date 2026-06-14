//! Voice recording via Web API (MediaRecorder) and waveform generation.
//!
//! Uses `web_sys::MediaRecorder` on WASM. On native (Tauri), the same
//! MediaRecorder API is available through the webview.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    BlobEvent, MediaRecorder, MediaRecorderOptions, MediaStream,
    MediaStreamConstraints, MediaStreamTrack,
};

/// A voice recording captured via the browser's MediaRecorder API.
#[derive(Debug, Clone)]
pub struct VoiceRecording {
    /// Raw audio bytes (Opus in WebM container).
    pub bytes: Vec<u8>,
    /// MIME type (usually "audio/webm;codecs=opus").
    pub mime: String,
    /// Duration in milliseconds.
    pub duration_ms: u32,
    /// Waveform data: 64 bars, each 0..255.
    pub waveform: Vec<u8>,
}

/// Audio recorder using `MediaRecorder` with `audio/webm;codecs=opus`.
pub struct Recorder {
    media: MediaStream,
    recorder: MediaRecorder,
    chunks: Rc<RefCell<Vec<u8>>>,
    start_time: f64,
}

impl Recorder {
    /// Request microphone permission and start recording.
    ///
    /// # Errors
    ///
    /// Returns a string description on failure (permission denied, etc.).
    pub async fn start() -> Result<Self, String> {
        let nav = web_sys::window()
            .ok_or("no window")?
            .navigator();
        let media_devices = nav.media_devices().map_err(|e| format!("no media devices: {e:?}"))?;

        let mut constraints = MediaStreamConstraints::new();
        constraints.audio(&JsValue::TRUE);
        constraints.video(&JsValue::FALSE);

        let stream_promise = media_devices
            .get_user_media_with_constraints(&constraints)
            .map_err(|e| format!("getUserMedia error: {e:?}"))?;
        let stream: MediaStream = JsFuture::from(stream_promise)
            .await
            .map_err(|e| format!("user denied or error: {e:?}"))?
            .dyn_into()
            .map_err(|_| "expected MediaStream".to_string())?;

        let mut opts = MediaRecorderOptions::new();
        opts.mime_type("audio/webm;codecs=opus");

        let recorder = MediaRecorder::new_with_media_stream_and_media_recorder_options(&stream, &opts)
            .map_err(|e| format!("MediaRecorder create: {e:?}"))?;

        let chunks: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(Vec::new()));
        let chunks_cb = chunks.clone();

        let on_data = Closure::<dyn FnMut(BlobEvent)>::new(move |e: BlobEvent| {
            if let Some(blob) = e.data() {
                let cc = chunks_cb.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(arr_buf) = JsFuture::from(blob.array_buffer()).await {
                        let u8a = Uint8Array::new(&arr_buf);
                        cc.borrow_mut().extend_from_slice(&u8a.to_vec());
                    }
                });
            }
        });
        recorder.set_ondataavailable(Some(on_data.as_ref().unchecked_ref()));
        on_data.forget();

        let start_time = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);

        recorder.start_with_time_slice(250).map_err(|e| format!("recorder start: {e:?}"))?;

        Ok(Self {
            media: stream,
            recorder,
            chunks,
            start_time,
        })
    }

    /// Stop recording and return the captured audio.
    pub async fn stop(self) -> VoiceRecording {
        self.recorder.stop().ok();

        // Wait for remaining dataavailable callbacks
        gloo_timers::future::TimeoutFuture::new(300).await;

        // Stop all tracks
        for track in self.media.get_tracks() {
            let t: MediaStreamTrack = track.dyn_into().unwrap();
            t.stop();
        }

        let bytes = self.chunks.borrow().clone();
        let duration_ms = (web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
            - self.start_time) as u32;

        let waveform = generate_waveform_from_audio(&bytes, 64).await;

        VoiceRecording {
            bytes,
            mime: "audio/webm;codecs=opus".into(),
            duration_ms,
            waveform,
        }
    }

    /// Cancel recording (discard data).
    pub fn cancel(self) {
        self.recorder.stop().ok();
        for track in self.media.get_tracks() {
            let t: MediaStreamTrack = track.dyn_into().unwrap();
            t.stop();
        }
    }
}

/// Generate a waveform (64 bars, 0..255) from Opus/WebM audio bytes.
///
/// Decodes via `AudioContext::decodeAudioData` and computes RMS per bin.
pub async fn generate_waveform_from_audio(bytes: &[u8], bars: usize) -> Vec<u8> {
    let ctx = match web_sys::AudioContext::new() {
        Ok(c) => c,
        Err(_) => return vec![128; bars], // fallback: flat waveform
    };

    let u8a = Uint8Array::from(bytes);
    let array_buffer = u8a.buffer();

    let decoded = match JsFuture::from(ctx.decode_audio_data(&array_buffer).unwrap()).await {
        Ok(buf) => buf.dyn_into::<web_sys::AudioBuffer>().unwrap(),
        Err(_) => return vec![128; bars],
    };

    let channel = decoded.get_channel_data(0).unwrap();
    let len = channel.len() as usize;
    if len == 0 {
        return vec![128; bars];
    }

    let bin_size = len / bars;
    let mut result = Vec::with_capacity(bars);
    for b in 0..bars {
        let start = b * bin_size;
        let end = ((b + 1) * bin_size).min(len);
        let mut sum_sq = 0.0f32;
        for i in start..end {
            if let Some(&v) = channel.get(i) {
                sum_sq += v * v;
            }
        }
        let rms = (sum_sq / (end - start) as f32).sqrt();
        result.push((rms.min(1.0) * 255.0) as u8);
    }
    result
}
