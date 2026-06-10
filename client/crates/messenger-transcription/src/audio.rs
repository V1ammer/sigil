//! Audio-side helpers — Whisper expects PCM 16 kHz mono `f32`.
//!
//! Decoding webm/opus is delegated to the WebView's `AudioContext` on the
//! WASM frontend (it already ships an Opus decoder for `<audio>` playback);
//! Rust only takes the resulting `f32` samples + their source sample rate
//! and resamples to 16 kHz with rubato if needed.

use crate::error::TranscriptionError;

const TARGET_RATE: u32 = 16_000;

/// Resample `samples` from `in_rate` to 16 kHz if they're not already there.
pub fn to_16k_mono(samples: Vec<f32>, in_rate: u32) -> Result<Vec<f32>, TranscriptionError> {
    if in_rate == TARGET_RATE || samples.is_empty() {
        return Ok(samples);
    }
    resample(&samples, in_rate, TARGET_RATE)
}

fn resample(input: &[f32], in_rate: u32, out_rate: u32) -> Result<Vec<f32>, TranscriptionError> {
    use rubato::{FftFixedIn, Resampler};

    let mut resampler =
        FftFixedIn::<f32>::new(in_rate as usize, out_rate as usize, input.len(), 1, 1)
            .map_err(|e| TranscriptionError::AudioDecode(e.to_string()))?;
    let waves_in = vec![input.to_vec()];
    let waves_out = resampler
        .process(&waves_in, None)
        .map_err(|e| TranscriptionError::AudioDecode(e.to_string()))?;
    Ok(waves_out.into_iter().next().unwrap_or_default())
}
