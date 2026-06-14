//! Message input bar — auto-resizing textarea, reply/edit preview, send/mic toggle,
//! recording UI, and attach menu.
use leptos::prelude::*;
use leptos::ev::KeyboardEvent;
use leptos::task::spawn_local;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;
use wasm_bindgen::prelude::JsCast;
use crate::i18n::{Language, t};
use crate::icons::Icon;
use crate::components::textarea::Textarea;
use crate::components::dropdown_menu::{DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem};
use crate::components::tooltip::Tooltip;
use crate::state::notifications::{NotificationsState, ToastKind};

#[cfg(feature = "voice")]
use messenger_core::voice::Recorder;

/// Raw voice recording produced by holding the mic button.
#[derive(Clone, Debug)]
pub struct VoicePayload {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub duration_ms: u32,
    pub waveform: Vec<u8>,
}

/// Raw attachment payload picked through the attach menu.
#[derive(Clone, Debug)]
pub struct AttachmentPayload {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub name: String,
    pub size: u64,
    /// Whether the user picked an image (kind hint for the envelope).
    pub is_image: bool,
}

/// A picked attachment held in the composer until the user presses send.
/// `preview_url` is an object URL for an image thumbnail (None for non-images).
#[derive(Clone)]
pub struct StagedAttachment {
    pub payload: AttachmentPayload,
    pub preview_url: Option<String>,
}

/// Stage an attachment into the per-chat store, building an image preview URL
/// and revoking any previously staged item for that chat. Shared by the
/// in-composer picker and the Android "share into chat" flow.
pub fn stage_into(
    staged: RwSignal<HashMap<Uuid, StagedAttachment>>,
    group_id: Uuid,
    payload: AttachmentPayload,
) {
    let preview_url = if payload.is_image {
        object_url_from_bytes(&payload.bytes, &payload.mime)
    } else {
        None
    };
    staged.update(|m| {
        if let Some(old) = m.insert(group_id, StagedAttachment { payload, preview_url }) {
            if let Some(url) = old.preview_url {
                let _ = web_sys::Url::revoke_object_url(&url);
            }
        }
    });
}

/// Build a blob object URL from raw bytes for an inline preview thumbnail.
fn object_url_from_bytes(bytes: &[u8], mime: &str) -> Option<String> {
    let u8a = js_sys::Uint8Array::from(bytes);
    let arr = js_sys::Array::new();
    arr.push(&u8a.into());
    let mut bag = web_sys::BlobPropertyBag::new();
    bag.type_(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &bag).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

/// Command queued while `Recorder::start()` is still in flight.
#[cfg(feature = "voice")]
#[derive(Clone, Copy)]
enum PendingVoiceCmd {
    Stop,
    Cancel,
}

#[cfg(feature = "voice")]
thread_local! {
    /// Active recorder for the currently held mic button. There can be at most one.
    /// Stored thread-local because `Recorder` is `!Send` (holds `Rc<RefCell<…>>` for chunks).
    static ACTIVE_RECORDER: RefCell<Option<Recorder>> = const { RefCell::new(None) };
    /// If the user releases the mic button while `Recorder::start()` is still
    /// awaiting microphone permission, the stop/cancel is queued here and the
    /// start task picks it up as soon as the recorder is ready.
    static PENDING_VOICE_CMD: RefCell<Option<PendingVoiceCmd>> = const { RefCell::new(None) };
}

/// Minimum duration for a voice message to be sent. Releases under this
/// threshold are treated as accidental taps and discarded.
#[cfg(feature = "voice")]
const VOICE_MIN_DURATION_MS: u32 = 400;

/// Preview mode for the input bar.
#[derive(Clone, Default)]
pub enum InputPreview {
    #[default]
    None,
    Reply {
        message_id: String,
        sender_name: String,
        content: String,
    },
    Edit {
        message_id: String,
        content: String,
    },
}

/// Typing indicators on? (privacy setting, default on.) Gates whether we
/// announce our own typing over the WebSocket.
fn typing_setting_on() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("ms_settings_typing_indicators").ok().flatten())
        .map_or(true, |v| v != "false")
}

#[must_use]
#[component]
pub fn InputBar(
    #[prop(optional, into)] locale: Signal<Language>,
    #[prop(optional, into)] preview: InputPreview,
    #[prop(optional)] on_send: Option<Box<dyn Fn(String) + Send + Sync + 'static>>,
    #[prop(optional)] on_send_voice: Option<Box<dyn Fn(VoicePayload) + Send + Sync + 'static>>,
    #[prop(optional)] on_send_attachment: Option<Box<dyn Fn(AttachmentPayload) + Send + Sync + 'static>>,
    #[prop(optional)] on_cancel_preview: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    /// Current chat — key for the per-chat draft.
    #[prop(optional)] group_id: Option<Uuid>,
    /// Persistent per-chat draft store, owned by the chat screen. Survives the
    /// re-renders that rebuild this component (incoming messages bump the chat
    /// list, which rebuilds the whole chat view including the InputBar).
    #[prop(optional)] drafts: Option<RwSignal<HashMap<Uuid, String>>>,
    /// Persistent per-chat staged attachment (picked but not yet sent). Owned
    /// by the chat screen for the same rebuild-survival reason as `drafts`.
    #[prop(optional)] staged: Option<RwSignal<HashMap<Uuid, StagedAttachment>>>,
) -> impl IntoView {
    // Seed the text from the saved draft so a rebuild doesn't wipe it.
    let initial_draft = match (drafts, group_id) {
        (Some(d), Some(gid)) => d.with_untracked(|m| m.get(&gid).cloned()).unwrap_or_default(),
        _ => String::new(),
    };
    let text = RwSignal::new(initial_draft.clone());

    // Persist / clear the draft for this chat.
    let save_draft = move |val: &str| {
        if let (Some(d), Some(gid)) = (drafts, group_id) {
            let v = val.to_string();
            d.update(|m| {
                if v.trim().is_empty() {
                    m.remove(&gid);
                } else {
                    m.insert(gid, v);
                }
            });
        }
    };
    let clear_draft = move || {
        if let (Some(d), Some(gid)) = (drafts, group_id) {
            d.update(|m| {
                m.remove(&gid);
            });
        }
    };
    let is_recording = RwSignal::new(false);
    let recording_duration = RwSignal::new(0u32);
    let recording_timer_id: RwSignal<Option<i32>> = RwSignal::new(None);

    // Auto-resize textarea reference
    let textarea_ref: NodeRef<leptos::html::Textarea> = NodeRef::new();

    // Hidden file inputs — separate so we can set distinct `accept` attributes.
    let photo_input_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    let file_input_ref: NodeRef<leptos::html::Input> = NodeRef::new();

    let on_send_attachment_arc = on_send_attachment.map(Arc::new);

    // Picking/pasting an attachment STAGES it in the composer (preview) instead
    // of sending immediately — it's sent only when the user presses send. Falls
    // back to immediate send if no staging store was provided.
    let stage_attachment: Arc<dyn Fn(AttachmentPayload) + Send + Sync> = {
        let on_send_attachment = on_send_attachment_arc.clone();
        Arc::new(move |payload: AttachmentPayload| {
            let (Some(staged), Some(gid)) = (staged, group_id) else {
                if let Some(f) = on_send_attachment.as_ref() {
                    f(payload);
                }
                return;
            };
            let preview_url = if payload.is_image {
                object_url_from_bytes(&payload.bytes, &payload.mime)
            } else {
                None
            };
            staged.update(|m| {
                if let Some(old) = m.insert(gid, StagedAttachment { payload, preview_url }) {
                    if let Some(url) = old.preview_url {
                        let _ = web_sys::Url::revoke_object_url(&url);
                    }
                }
            });
        })
    };
    // Remove the staged attachment for this chat (revoking its preview URL).
    let clear_staged: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let (Some(staged), Some(gid)) = (staged, group_id) else { return };
        staged.update(|m| {
            if let Some(old) = m.remove(&gid) {
                if let Some(url) = old.preview_url {
                    let _ = web_sys::Url::revoke_object_url(&url);
                }
            }
        });
    });
    let stage_for_paste = stage_attachment.clone();

    // Trigger native file picker by clicking the hidden input.
    let click_hidden = |node: NodeRef<leptos::html::Input>| {
        if let Some(el) = node.get() {
            let inp: &web_sys::HtmlInputElement = el.unchecked_ref();
            inp.click();
        }
    };
    let on_attach_photo_cb = {
        let photo_input_ref = photo_input_ref.clone();
        Box::new(move || click_hidden(photo_input_ref))
            as Box<dyn Fn() + Send + Sync + 'static>
    };
    let on_attach_file_cb = {
        let file_input_ref = file_input_ref.clone();
        Box::new(move || click_hidden(file_input_ref))
            as Box<dyn Fn() + Send + Sync + 'static>
    };

    // Common file→bytes→stage bridge for both inputs.
    let make_on_change = move |is_image: bool| {
        let stage = stage_attachment.clone();
        move |ev: leptos::ev::Event| {
            let stage = stage.clone();
            let target = match ev.target() {
                Some(t) => t,
                None => return,
            };
            let input: web_sys::HtmlInputElement = target.unchecked_into();
            let files = match input.files() {
                Some(f) => f,
                None => return,
            };
            let Some(file) = files.get(0) else { return };
            // Reset so picking the same file twice still fires `change`.
            input.set_value("");
            let name = file.name();
            let size = file.size() as u64;
            let mime = file.type_();
            spawn_local(async move {
                let buf_promise = file.array_buffer();
                let buf_js = match wasm_bindgen_futures::JsFuture::from(buf_promise).await {
                    Ok(v) => v,
                    Err(e) => {
                        web_sys::console::error_1(&format!("file read: {e:?}").into());
                        return;
                    }
                };
                let arr_buf: js_sys::ArrayBuffer = buf_js.unchecked_into();
                let bytes = js_sys::Uint8Array::new(&arr_buf).to_vec();
                stage(AttachmentPayload {
                    bytes,
                    mime: if mime.is_empty() { "application/octet-stream".into() } else { mime },
                    name,
                    size,
                    is_image,
                });
            });
        }
    };

    let on_photo_change = make_on_change(true);
    let on_file_change = make_on_change(false);

    // Paste-to-attach: Ctrl+V an image straight into the composer. Handled
    // through the `Textarea`'s own `on:paste` so Leptos owns the listener
    // (no leaked native handler that would fire once per re-render).
    let on_paste_cb = {
        let stage = stage_for_paste.clone();
        Box::new(move |ev: web_sys::ClipboardEvent| {
            let Some(dt) = ev.clipboard_data() else { return };
            let items = dt.items();
            for i in 0..items.length() {
                let Some(item) = items.get(i) else { continue };
                if !item.type_().starts_with("image/") {
                    continue;
                }
                // Pasting an image shouldn't also drop its path/text into the box.
                ev.prevent_default();
                let Ok(Some(file)) = item.get_as_file() else { continue };
                let mime = file.type_();
                let size = file.size() as u64;
                // Clipboard images usually have no name — synthesize one.
                let name = {
                    let n = file.name();
                    if n.is_empty() {
                        let ext = mime.rsplit('/').next().unwrap_or("png");
                        format!("pasted-{}.{ext}", js_sys::Date::now() as u64)
                    } else {
                        n
                    }
                };
                let stage = stage.clone();
                spawn_local(async move {
                    let buf_js = match wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            web_sys::console::error_1(&format!("paste image read: {e:?}").into());
                            return;
                        }
                    };
                    let arr_buf: js_sys::ArrayBuffer = buf_js.unchecked_into();
                    let bytes = js_sys::Uint8Array::new(&arr_buf).to_vec();
                    {
                        stage(AttachmentPayload {
                            bytes,
                            mime: if mime.is_empty() { "image/png".into() } else { mime },
                            name,
                            size,
                            is_image: true,
                        });
                    }
                });
                break;
            }
        }) as Box<dyn Fn(web_sys::ClipboardEvent) + Send + Sync + 'static>
    };

    let has_text = move || !text.get().trim().is_empty();

    let on_send_arc = on_send.map(Arc::new);
    let on_send_for_keydown = on_send_arc.clone();

    // ── Typing indicator emission ──────────────────────────────────────
    // Announce typing while the user edits, throttled, with a debounced
    // "stopped" that also fires immediately on send. Gated by the privacy
    // setting; no-op until we know which chat we're in.
    let typing_group = group_id;
    let typing_last_at = StoredValue::new(0.0_f64);
    let typing_stop_token = StoredValue::new(0u64);
    let emit_typing_start: Arc<dyn Fn() + Send + Sync> = {
        let ws = use_context::<crate::state::ws_manager::WsManager>();
        Arc::new(move || {
            let (Some(ws), Some(gid)) = (ws.clone(), typing_group) else { return };
            if !typing_setting_on() {
                return;
            }
            let now = js_sys::Date::now();
            if now - typing_last_at.get_value() > 2500.0 {
                ws.send_typing(gid, true);
                typing_last_at.set_value(now);
            }
            // Debounced stop: only the latest keystroke's timer wins.
            let token = typing_stop_token.get_value().wrapping_add(1);
            typing_stop_token.set_value(token);
            let ws_stop = ws.clone();
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(3000).await;
                if typing_stop_token.get_value() == token {
                    ws_stop.send_typing(gid, false);
                    typing_last_at.set_value(0.0);
                }
            });
        })
    };
    let emit_typing_stop: Arc<dyn Fn() + Send + Sync> = {
        let ws = use_context::<crate::state::ws_manager::WsManager>();
        Arc::new(move || {
            let (Some(ws), Some(gid)) = (ws.clone(), typing_group) else { return };
            // Invalidate any pending debounced stop, then stop now.
            typing_stop_token.set_value(typing_stop_token.get_value().wrapping_add(1));
            if typing_last_at.get_value() != 0.0 {
                ws.send_typing(gid, false);
                typing_last_at.set_value(0.0);
            }
        })
    };
    let emit_stop_for_send = emit_typing_stop.clone();
    let emit_stop_for_keydown = emit_typing_stop.clone();

    // Send the chat's staged attachment (if any), then clear it.
    let send_staged: Arc<dyn Fn() + Send + Sync> = {
        let on_send_attachment = on_send_attachment_arc.clone();
        let clear_staged = clear_staged.clone();
        Arc::new(move || {
            let payload = match (staged, group_id) {
                (Some(s), Some(g)) => s.with_untracked(|m| m.get(&g).map(|st| st.payload.clone())),
                _ => None,
            };
            if let Some(payload) = payload {
                if let Some(f) = on_send_attachment.as_ref() {
                    f(payload);
                }
                clear_staged();
            }
        })
    };
    // Reactive: is an attachment staged for this chat? (drives send-button.)
    let has_staged = move || match (staged, group_id) {
        (Some(s), Some(g)) => s.with(|m| m.contains_key(&g)),
        _ => false,
    };
    let send_staged_for_send = send_staged.clone();
    let send_staged_for_keydown = send_staged.clone();
    let clear_staged_for_preview = clear_staged.clone();

    let handle_send = move |()| {
        // Staged attachment goes first, then any typed text/caption.
        send_staged_for_send();
        let msg = text.get().trim().to_string();
        if !msg.is_empty() {
            if let Some(ref f) = on_send_arc {
                f(msg);
            }
            text.set(String::new());
            clear_draft();
            // Clear textarea value via node_ref
            if let Some(el) = textarea_ref.get() {
                let textarea: &web_sys::HtmlTextAreaElement = el.unchecked_ref();
                textarea.set_value("");
                let _ = textarea.set_attribute("style", "height: auto");
            }
        }
        emit_stop_for_send();
    };

    let handle_send = Arc::new(handle_send);

    let handle_key_down = move |ev: KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            let msg = text.get().trim().to_string();
            // Enter sends when there's text OR a staged attachment.
            if msg.is_empty() && !has_staged() {
                return;
            }
            ev.prevent_default();
            send_staged_for_keydown();
            if !msg.is_empty() {
                if let Some(ref f) = on_send_for_keydown {
                    f(msg);
                }
                text.set(String::new());
                clear_draft();
                if let Some(el) = textarea_ref.get() {
                    let textarea: &web_sys::HtmlTextAreaElement = el.unchecked_ref();
                    textarea.set_value("");
                    let _ = textarea.set_attribute("style", "height: auto");
                }
            }
            emit_stop_for_keydown();
        }
    };

    let handle_change = move |val: String| {
        if val.trim().is_empty() {
            emit_typing_stop();
        } else {
            emit_typing_start();
        }
        save_draft(&val);
        text.set(val);
        // Auto-resize
        if let Some(el) = textarea_ref.get() {
            let textarea: &web_sys::HtmlTextAreaElement = el.unchecked_ref();
            let _ = textarea.set_attribute("style", "height: auto");
            let scroll_height = textarea.scroll_height();
            let new_height = scroll_height.min(120).max(36);
            let _ = textarea.set_attribute("style", &format!("height: {}px", new_height));
        }
    };

    // Voice recording – hold to record, release to send.
    // Pipeline:
    //   pointerdown → mark recording, start duration ticker, start MediaRecorder (async)
    //   pointerup   → stop MediaRecorder, await chunks, hand VoicePayload to callback
    //   cancel      → drop MediaRecorder without invoking callback
    let on_send_voice_arc = on_send_voice.map(Arc::new);
    let recording_timer_id: RwSignal<Option<i32>> = RwSignal::new(None);

    let stop_duration_ticker = move || {
        if let Some(id) = recording_timer_id.get_untracked() {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(id);
            }
            recording_timer_id.set(None);
        }
    };

    let start_recording_core = std::sync::Arc::new({
        let on_send_voice = on_send_voice_arc.clone();
        move || {
            if is_recording.get_untracked() { return; }
            is_recording.set(true);
            recording_duration.set(0);
            // Duration ticker — independent of the underlying MediaRecorder so the UI
            // shows progress even while we're awaiting microphone permission.
            let dur = recording_duration;
            let callback = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                dur.update(|d| *d += 1);
            }) as Box<dyn FnMut()>);
            if let Some(window) = web_sys::window() {
                if let Ok(id) = window.set_interval_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    1_000,
                ) {
                    recording_timer_id.set(Some(id));
                    callback.forget(); // Leaks one closure per recording session.
                }
            }
            #[cfg(feature = "voice")]
            {
                // Clear any stale pending command from a previous session.
                PENDING_VOICE_CMD.with(|c| *c.borrow_mut() = None);
                let on_send_voice = on_send_voice.clone();
                spawn_local(async move {
                    let rec = match Recorder::start().await {
                        Ok(rec) => rec,
                        Err(e) => {
                            web_sys::console::error_1(&format!("[InputBar] Recorder::start: {e}").into());
                            is_recording.set(false);
                            PENDING_VOICE_CMD.with(|c| *c.borrow_mut() = None);
                            if let Some(notif) = use_context::<NotificationsState>() {
                                let lower = e.to_lowercase();
                                let msg = if lower.contains("notallowed") || lower.contains("permission") || lower.contains("denied") {
                                    "Доступ к микрофону запрещён. Разрешите его в настройках браузера/приложения."
                                } else {
                                    "Не удалось включить микрофон"
                                };
                                notif.push(ToastKind::Error, msg);
                            }
                            return;
                        }
                    };
                    // The user may have already released / cancelled while we
                    // were awaiting microphone permission. Pick that up here.
                    let pending = PENDING_VOICE_CMD.with(|c| c.borrow_mut().take());
                    match pending {
                        Some(PendingVoiceCmd::Cancel) => {
                            rec.cancel();
                        }
                        Some(PendingVoiceCmd::Stop) => {
                            let recording = rec.stop().await;
                            if !recording.bytes.is_empty()
                                && recording.duration_ms >= VOICE_MIN_DURATION_MS
                            {
                                if let Some(f) = on_send_voice.as_ref() {
                                    f(VoicePayload {
                                        bytes: recording.bytes,
                                        mime: recording.mime,
                                        duration_ms: recording.duration_ms,
                                        waveform: recording.waveform,
                                    });
                                }
                            }
                        }
                        None => {
                            ACTIVE_RECORDER.with(|cell| *cell.borrow_mut() = Some(rec));
                        }
                    }
                });
            }
            #[cfg(not(feature = "voice"))]
            { let _ = on_send_voice.clone(); }
        }
    }) as std::sync::Arc<dyn Fn() + Send + Sync + 'static>;

    let stop_recording_core = std::sync::Arc::new({
        let on_send_voice = on_send_voice_arc.clone();
        move || {
            if !is_recording.get_untracked() { return; }
            is_recording.set(false);
            stop_duration_ticker();
            recording_duration.set(0);
            #[cfg(feature = "voice")]
            {
                let on_send_voice = on_send_voice.clone();
                spawn_local(async move {
                    let rec_opt = ACTIVE_RECORDER.with(|cell| cell.borrow_mut().take());
                    let Some(rec) = rec_opt else {
                        // Recorder hasn't started yet — queue a stop so the
                        // start task finishes the recording and sends.
                        PENDING_VOICE_CMD.with(|c| *c.borrow_mut() = Some(PendingVoiceCmd::Stop));
                        return;
                    };
                    let recording = rec.stop().await;
                    if recording.bytes.is_empty()
                        || recording.duration_ms < VOICE_MIN_DURATION_MS
                    {
                        web_sys::console::warn_1(&"[InputBar] short/empty recording, skip send".into());
                        return;
                    }
                    if let Some(f) = on_send_voice.as_ref() {
                        f(VoicePayload {
                            bytes: recording.bytes,
                            mime: recording.mime,
                            duration_ms: recording.duration_ms,
                            waveform: recording.waveform,
                        });
                    }
                });
            }
            #[cfg(not(feature = "voice"))]
            { let _ = on_send_voice.clone(); }
        }
    }) as std::sync::Arc<dyn Fn() + Send + Sync + 'static>;

    let cancel_recording_core = std::sync::Arc::new({
        move || {
            if !is_recording.get_untracked() { return; }
            is_recording.set(false);
            stop_duration_ticker();
            #[cfg(feature = "voice")]
            {
                if let Some(rec) = ACTIVE_RECORDER.with(|cell| cell.borrow_mut().take()) {
                    rec.cancel();
                } else {
                    // Recorder still starting — queue a cancel so it's torn
                    // down as soon as it's ready.
                    PENDING_VOICE_CMD.with(|c| *c.borrow_mut() = Some(PendingVoiceCmd::Cancel));
                }
            }
            recording_duration.set(0);
        }
    }) as std::sync::Arc<dyn Fn() + Send + Sync + 'static>;

    let should_cancel = RwSignal::new(false);
    let touch_start_x = RwSignal::new(0.0);

    // Pre-defined handlers for the voice button (avoids DOM swap issues, keeps outer FnMut)
    let on_voice_pointerdown = {
        let should_cancel = should_cancel.clone();
        let touch_start_x = touch_start_x.clone();
        let start = start_recording_core.clone();
        move |e: leptos::ev::PointerEvent| {
            e.prevent_default();
            if let Some(target) = e.target() {
                let el: web_sys::HtmlElement = target.unchecked_into();
                let _ = el.set_pointer_capture(e.pointer_id());
            }
            should_cancel.set(false);
            touch_start_x.set(e.client_x() as f64);
            start();
        }
    };
    let on_voice_pointermove = {
        let should_cancel = should_cancel.clone();
        let touch_start_x = touch_start_x.clone();
        let is_recording = is_recording.clone();
        move |e: leptos::ev::PointerEvent| {
            if is_recording.get() {
                let dx = e.client_x() as f64 - touch_start_x.get();
                should_cancel.set(dx < -40.0);
            }
        }
    };
    let on_voice_pointerup = {
        let should_cancel = should_cancel.clone();
        let is_recording = is_recording.clone();
        let stop = stop_recording_core.clone();
        let cancel = cancel_recording_core.clone();
        move |_| {
            if is_recording.get() {
                if should_cancel.get() {
                    cancel();
                } else {
                    stop();
                }
                should_cancel.set(false);
            }
        }
    };
    let on_voice_pointercancel = {
        let should_cancel = should_cancel.clone();
        let cancel = cancel_recording_core.clone();
        move |_| {
            cancel();
            should_cancel.set(false);
        }
    };
    let on_recording_cancel = {
        let cancel = cancel_recording_core.clone();
        move |_| cancel()
    };

    let on_change_cb = Box::new(handle_change) as Box<dyn Fn(String) + Send + Sync + 'static>;
    let on_key_down_cb = Box::new(handle_key_down) as Box<dyn Fn(KeyboardEvent) + Send + Sync + 'static>;

    view! {
        <div class="border-t border-border bg-card px-2 py-1.5">
            // Hidden file inputs — opened by attach-menu items.
            <input
                node_ref=photo_input_ref
                type="file"
                accept="image/*"
                style="display:none"
                on:change=on_photo_change
            />
            <input
                node_ref=file_input_ref
                type="file"
                style="display:none"
                on:change=on_file_change
            />
            // Preview bar (reply or edit indicator)
            {match &preview {
                InputPreview::Reply { sender_name, content, .. } => {
                    view! {
                        <div class="flex items-center gap-2 mb-1.5 px-2 py-1 rounded-lg bg-accent/30 border-l-2 border-primary">
                            <div class="flex items-center gap-1.5 min-w-0 flex-1">
                                <Icon name="reply" class_name="h-4 w-4 shrink-0 text-primary"/>
                                <div class="min-w-0">
                                    <p class="text-xs font-medium text-primary truncate">{sender_name.clone()}</p>
                                    <p class="text-xs text-muted-foreground line-clamp-1">{content.clone()}</p>
                                </div>
                            </div>
                            <button
                                class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md hover:bg-accent"
                                on:click=move |_| { if let Some(f) = on_cancel_preview.as_ref() { f(); } }
                            >
                                <Icon name="x" class_name="h-3.5 w-3.5"/>
                            </button>
                        </div>
                    }.into_any()
                }
                InputPreview::Edit { content, .. } => {
                    view! {
                        <div class="flex items-center gap-2 mb-1.5 px-2 py-1 rounded-lg bg-accent/30 border-l-2 border-warning">
                            <div class="flex items-center gap-1.5 min-w-0 flex-1">
                                <Icon name="edit" class_name="h-4 w-4 shrink-0 text-warning"/>
                                <div class="min-w-0">
                                    <p class="text-xs font-medium text-warning-foreground">{t(locale.get(), "message.editing")}</p>
                                    <p class="text-xs text-muted-foreground line-clamp-1">{content.clone()}</p>
                                </div>
                            </div>
                            <button
                                class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md hover:bg-accent"
                                on:click=move |_| { if let Some(f) = on_cancel_preview.as_ref() { f(); } }
                            >
                                <Icon name="x" class_name="h-3.5 w-3.5"/>
                            </button>
                        </div>
                    }.into_any()
                }
                InputPreview::None => view! {}.into_any(),
            }}

            // Staged attachment preview — held here until the user presses send.
            {move || {
                let st = match (staged, group_id) {
                    (Some(s), Some(g)) => s.with(|m| m.get(&g).cloned()),
                    _ => None,
                };
                st.map(|st| {
                    let clear = clear_staged_for_preview.clone();
                    let name = st.payload.name.clone();
                    let thumb = st.preview_url.clone();
                    view! {
                        <div class="flex items-center gap-2 mb-1.5 px-2 py-1.5 rounded-lg bg-accent/30 border-l-2 border-primary">
                            {match thumb {
                                Some(url) => view! {
                                    <img src=url class="h-12 w-12 shrink-0 rounded object-cover"/>
                                }.into_any(),
                                None => view! {
                                    <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded bg-muted">
                                        <Icon name="paperclip" class_name="h-5 w-5 text-muted-foreground"/>
                                    </div>
                                }.into_any(),
                            }}
                            <span class="min-w-0 flex-1 truncate text-sm text-foreground">{name}</span>
                            <button
                                class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md hover:bg-accent"
                                on:click=move |_| clear()
                            >
                                <Icon name="x" class_name="h-3.5 w-3.5"/>
                            </button>
                        </div>
                    }
                })
            }}

            // Recording UI
            {move || {
                if !is_recording.get() { return None; }
                let on_cancel_click = on_recording_cancel.clone();
                let dur = move || format!("{}:{:02}", recording_duration.get() / 60, recording_duration.get() % 60);
                Some(view! {
                    <div class="flex items-center gap-2 mb-1.5 px-2 py-1.5 rounded-lg bg-destructive/10">
                        <div class="flex items-center gap-2">
                            <span class="h-3 w-3 animate-pulse rounded-full bg-destructive"/>
                            <span class="text-sm font-medium text-destructive-foreground">{t(locale.get(), "message.recording")}</span>
                        </div>
                        <span class="text-sm tabular-nums text-muted-foreground">
                            {dur}
                        </span>
                        <div class="flex-1 flex items-center gap-px h-5">
                            {(0..30).map(|_| {
                                let h = (fast_rand() * 80.0 + 10.0) as u32;
                                view! {
                                    <div class="flex-1 rounded-full bg-destructive/40" style=format!("height:{h}%")/>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                        <button
                            class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground hover:bg-muted/80 transition-colors"
                            on:click=on_cancel_click
                        >
                            <Icon name="x" class_name="h-3.5 w-3.5"/>
                        </button>
                    </div>
                })
            }}

            // Input row
            <div class="flex items-end gap-1.5">
                // Attach button
                <DropdownMenu>
                    <DropdownMenuTrigger>
                        <Tooltip text={t(locale.get(), "attach.file")}>
                            <button class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md hover:bg-accent transition-colors">
                                <Icon name="paperclip" class_name="h-4 w-4 text-muted-foreground"/>
                            </button>
                        </Tooltip>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent class="min-w-[10rem]" align="start">
                        <DropdownMenuItem on_click=on_attach_photo_cb>
                            <Icon name="image" class_name="mr-2 h-4 w-4"/>
                            {t(locale.get(), "attach.photo")}
                        </DropdownMenuItem>
                        <DropdownMenuItem on_click=on_attach_file_cb>
                            <Icon name="file" class_name="mr-2 h-4 w-4"/>
                            {t(locale.get(), "attach.file")}
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>

                // Textarea
                <div class="flex-1 relative">
                    <Textarea
                        placeholder={t(locale.get(), "message.placeholder")}
                        class="min-h-[36px] max-h-[120px] resize-none py-1.5 text-sm"
                        rows=1u32
                        value=initial_draft
                        on_change=on_change_cb
                        on_key_down=on_key_down_cb
                        on_paste=on_paste_cb
                        node_ref=textarea_ref
                    />
                </div>

            // Send / Mic button — stable element, pointer events with capture
            // so hold-to-record, release-to-send, and swipe-left-to-cancel all work
            // without DOM-swap issues.
            {move || {
                let handle_send = handle_send.clone();
                if has_text() || has_staged() {
                    view! {
                        <button
                            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                            on:click=move |_| handle_send(())
                        >
                            <Icon name="send" class_name="h-4 w-4"/>
                        </button>
                    }.into_any()
                } else {
                    let on_down = on_voice_pointerdown.clone();
                    let on_move = on_voice_pointermove.clone();
                    let on_up = on_voice_pointerup.clone();
                    let on_cancel = on_voice_pointercancel.clone();
                    view! {
                        <button
                            class=move || {
                                if is_recording.get() {
                                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-destructive text-destructive-foreground transition-colors"
                                } else {
                                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-md hover:bg-accent transition-colors active:bg-primary/20"
                                }
                            }
                            style="touch-action: none"
                            on:pointerdown=on_down
                            on:pointermove=on_move
                            on:pointerup=on_up
                            on:pointercancel=on_cancel
                        >
                            <Icon name="mic" class_name="h-4 w-4"/>
                        </button>
                    }.into_any()
                }
            }}
            </div>
        </div>
    }
}

/// Simple deterministic pseudo-random for waveform simulation.
fn fast_rand() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<f64> = const { Cell::new(0.5) };
    }
    SEED.with(|s| {
        let val = (s.get() * 1.618 + 0.5) % 1.0;
        s.set(val);
        val
    })
}
