//! Thin wrapper over whisper-rs: load a ggml model, run inference on PCM samples,
//! collect segments.

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::TranscriptionError;

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    /// Load a ggml whisper model from disk.
    pub fn load(model_path: &Path) -> Result<Self, TranscriptionError> {
        let path_str = model_path.to_str().ok_or(TranscriptionError::InvalidPath)?;
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(path_str, params)
            .map_err(|e| TranscriptionError::ModelLoad(e.to_string()))?;
        Ok(Self { ctx })
    }

    /// Transcribe 16 kHz mono `f32` samples in `[-1, 1]`.
    /// `language_hint` is an ISO 639-1 code (e.g. "en", "ru"); pass `None` to
    /// let Whisper auto-detect.
    pub fn transcribe(
        &self,
        samples: &[f32],
        language_hint: Option<&str>,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| TranscriptionError::Internal(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(language_hint);
        params.set_translate(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_n_threads(num_cpus::get() as i32);

        state
            .full(params, samples)
            .map_err(|e| TranscriptionError::Internal(e.to_string()))?;

        let n_segments = state
            .full_n_segments()
            .map_err(|e| TranscriptionError::Internal(e.to_string()))?;

        let mut full_text = String::new();
        let mut segments = Vec::with_capacity(n_segments as usize);
        for i in 0..n_segments {
            let text = state
                .full_get_segment_text(i)
                .map_err(|e| TranscriptionError::Internal(e.to_string()))?;
            let t0 = state.full_get_segment_t0(i).unwrap_or(0);
            let t1 = state.full_get_segment_t1(i).unwrap_or(0);
            if !full_text.is_empty() {
                full_text.push(' ');
            }
            full_text.push_str(text.trim());
            segments.push(TranscriptSegment {
                start_ms: (t0 * 10) as u32,
                end_ms: (t1 * 10) as u32,
                text: text.trim().to_string(),
            });
        }

        Ok(TranscriptionResult {
            text: full_text,
            segments,
        })
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TranscriptSegment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
}
