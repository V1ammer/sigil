//! WebSocket endpoint `/v1/ws`.
//!
//! Реализует:
//! - Upgrade HTTP → WebSocket.
//! - Handshake auth (Ed25519 signature challenge) с таймаутом 30 секунд.
//! - Двусторонний обмен `ServerFrame` / `ClientFrame` (msgpack binary, либо JSON text).
//! - Backpressure: bounded `mpsc::channel(256)`.
//! - Idle timeout: закрывает соединение после `websocket_idle_timeout_secs` бездействия.

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use futures_util::{SinkExt, StreamExt};

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::middleware::AuthContext;
use crate::error::AppError;
use crate::services::invite::now_secs;
use crate::state::AppState;
use crate::ws_registry::ServerFrame;
use messenger_entity::mls_group_members;

/// Фрейм от клиента серверу через WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    Auth {
        device_id: Uuid,
        timestamp: i64,
        #[serde(with = "serde_bytes")]
        nonce: Vec<u8>,
        #[serde(with = "serde_bytes")]
        signature: Vec<u8>,
    },
    Ping,
    Typing {
        group_id: Uuid,
        started: bool,
    },
}

/// WebSocket upgrade handler.
///
/// Не требует REST-аутентификации — auth происходит после upgrade.
pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Обработчик WebSocket-соединения.
#[allow(clippy::too_many_lines, clippy::unnested_or_patterns)]
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerFrame>(256);

    // 1. Ждём auth frame с таймаутом 30 секунд
    let auth_result = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if let Ok(ClientFrame::Auth {
                        device_id,
                        timestamp,
                        nonce,
                        signature,
                    }) = rmp_serde::from_slice::<ClientFrame>(&data)
                    {
                        return Some((device_id, timestamp, nonce, signature, false));
                    }
                }
                Ok(Message::Text(data)) => {
                    if let Ok(ClientFrame::Auth {
                        device_id,
                        timestamp,
                        nonce,
                        signature,
                    }) = serde_json::from_str::<ClientFrame>(&data)
                    {
                        return Some((device_id, timestamp, nonce, signature, true));
                    }
                }
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    })
    .await;

    let Ok(Some((device_id, ts, nonce, signature, use_json))) = auth_result else {
        let _ = sender
            .send(Message::Binary(
                rmp_serde::to_vec_named(&ServerFrame::AuthError {
                    code: "ERR_AUTH_TIMEOUT".into(),
                })
                .unwrap(),
            ))
            .await;
        return;
    };

    // 2. Валидация auth (как в S04 middleware)
    let auth_ctx = match validate_ws_auth(&state, device_id, ts, &nonce, &signature).await {
        Ok(ctx) => ctx,
        Err(e) => {
            let code = match e {
                AppError::TimestampOutOfWindow => "ERR_TIMESTAMP_OUT_OF_WINDOW",
                AppError::NonceReplay => "ERR_NONCE_REPLAY",
                AppError::DeviceRevoked => "ERR_DEVICE_REVOKED",
                AppError::Forbidden => "ERR_USER_INACTIVE",
                _ => "ERR_AUTH_FAILED",
            };
            let _ = sender
                .send(encode_frame(
                    &ServerFrame::AuthError {
                        code: code.into(),
                    },
                    use_json,
                ))
                .await;
            return;
        }
    };

    // 3. Регистрация в реестре (replace old connection if any)
    // Клонируем tx для reader'а (чтобы он мог отправлять Pong)
    let tx_for_reader = tx.clone();
    state
        .ws_registry
        .register(auth_ctx.user.id, auth_ctx.device.id, tx);

    // 4. Отправляем AuthOk
    let _ = sender
        .send(encode_frame(
            &ServerFrame::AuthOk {
                user_id: auth_ctx.user.id,
            },
            use_json,
        ))
        .await;

    let user_id = auth_ctx.user.id;
    let device_id = auth_ctx.device.id;
    let state_for_reader = state.clone();

    // 5. Writer task: rx → ws sender
    let write_task = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let bytes = encode_frame(&frame, use_json);
            if sender.send(bytes).await.is_err() {
                break;
            }
        }
    });

    // 6. Reader task: ws receiver → handle frames
    let read_task = tokio::spawn(async move {
        let idle_timeout = Duration::from_secs(
            state_for_reader
                .config
                .websocket_idle_timeout_secs,
        );
        loop {
            let next = tokio::time::timeout(idle_timeout, receiver.next()).await;
            match next {
                Ok(Some(Ok(msg))) => {
                    if handle_client_frame(
                        &state_for_reader,
                        &auth_ctx,
                        &tx_for_reader,
                        msg,
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
                Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
            }
        }
    });

    tokio::select! {
        _ = write_task => {},
        _ = read_task => {},
    }

    // 7. Очистка
    state.ws_registry.unregister(user_id, device_id);
}

/// Валидация auth-фрейма WebSocket.
///
/// Использует тот же `nonce_cache`, что и REST middleware.
///
/// # Errors
///
/// Возвращает `AppError` при ошибке аутентификации.
async fn validate_ws_auth(
    state: &AppState,
    device_id: Uuid,
    timestamp: i64,
    nonce: &[u8],
    signature: &[u8],
) -> Result<AuthContext, AppError> {
    // 1. Timestamp window
    let now = now_secs();
    let skew = state.config.clock_skew_tolerance_secs;
    if (timestamp - now).abs() > skew {
        return Err(AppError::TimestampOutOfWindow);
    }

    // 2. Nonce replay protection
    if state.nonce_cache.check_and_insert(nonce) {
        return Err(AppError::NonceReplay);
    }

    // 3. Загрузить device
    let device = messenger_entity::devices::Entity::find_by_id(device_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if device.revoked_at.is_some() {
        return Err(AppError::DeviceRevoked);
    }

    // 4. Подпись: "GET\n/v1/ws\n{ts}\n{nonce_hex}\n{blake3(empty)}"
    let empty_hash = blake3::hash(b"").to_string();
    let canonical = format!(
        "GET\n/v1/ws\n{ts}\n{nonce_hex}\n{empty_hash}",
        ts = timestamp,
        nonce_hex = hex::encode(nonce),
    );

    messenger_crypto::signing::verify_ed25519(
        &device.device_signing_public_key,
        canonical.as_bytes(),
        signature,
    )
    .map_err(|_| AppError::SignatureInvalid)?;

    // 5. Загрузить пользователя
    let user = messenger_entity::users::Entity::find_by_id(device.user_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.status != "active" {
        return Err(AppError::Forbidden);
    }

    Ok(AuthContext { device, user })
}

/// Обработка входящего фрейма от клиента.
///
/// # Errors
///
/// Возвращает ошибку если соединение должно быть закрыто.
async fn handle_client_frame(
    state: &AppState,
    ctx: &AuthContext,
    tx: &mpsc::Sender<ServerFrame>,
    msg: Message,
) -> Result<(), AppError> {
    let frame: ClientFrame = match msg {
        Message::Binary(data) => {
            rmp_serde::from_slice(&data)
                .map_err(|_| AppError::BadRequest("invalid frame".into()))?
        }
        Message::Text(data) => serde_json::from_str(&data)
            .map_err(|_| AppError::BadRequest("invalid frame".into()))?,
        // Ping/Pong автоматически обрабатываются axum на уровне tungstenite
        _ => return Ok(()),
    };

    match frame {
        ClientFrame::Ping => {
            let _ = tx.send(ServerFrame::Pong).await;
        }
        ClientFrame::Typing { group_id, started } => {
            handle_typing(state, ctx, group_id, started).await;
        }
        ClientFrame::Auth { .. } => {}
    }

    Ok(())
}

/// Обработка typing-индикатора.
///
/// Проверяет membership и broadcast'ит остальным активным членам группы.
async fn handle_typing(state: &AppState, ctx: &AuthContext, group_id: Uuid, started: bool) {
    // Проверить membership
    let Ok(Some(_is_member)) = mls_group_members::Entity::find()
        .filter(
            mls_group_members::Column::GroupId
                .eq(group_id)
                .and(mls_group_members::Column::UserId.eq(ctx.user.id))
                .and(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&state.db)
        .await
    else {
        return; // игнорируем если не member или ошибка БД
    };

    // Найти всех active members группы (кроме себя)
    let Ok(members) = mls_group_members::Entity::find()
        .filter(
            mls_group_members::Column::GroupId
                .eq(group_id)
                .and(mls_group_members::Column::UserId.ne(ctx.user.id))
                .and(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .all(&state.db)
        .await
    else {
        return;
    };

    let frame = ServerFrame::Typing {
        group_id,
        user_id: ctx.user.id,
        started,
    };

    for member in members {
        state.ws_registry.send_to_user(member.user_id, &frame);
    }
}

/// Кодирует `ServerFrame` в WebSocket `Message`.
fn encode_frame(frame: &ServerFrame, use_json: bool) -> Message {
    if use_json {
        Message::Text(serde_json::to_string(frame).unwrap_or_default())
    } else {
        Message::Binary(rmp_serde::to_vec_named(frame).unwrap_or_default())
    }
}

// ─── Notification helpers ───

/// Уведомляет всех участников группы о новом сообщении.
pub async fn notify_message_to_group(
    state: &AppState,
    group_id: Uuid,
    message_id: Uuid,
    epoch: i64,
) {
    let frame = ServerFrame::NewMessage {
        group_id,
        message_id,
        epoch,
    };

    // Найти всех активных членов группы
    let Ok(members) = mls_group_members::Entity::find()
        .filter(
            mls_group_members::Column::GroupId
                .eq(group_id)
                .and(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .all(&state.db)
        .await
    else {
        return;
    };

    for member in members {
        state
            .ws_registry
            .send_to_user(member.user_id, &frame);
    }
}

/// Уведомляет устройство о новом welcome'е.
pub async fn notify_welcome(
    state: &AppState,
    recipient_user_id: Uuid,
    recipient_device_id: Uuid,
    welcome_id: Uuid,
    group_id: Uuid,
) {
    let frame = ServerFrame::NewWelcome {
        welcome_id,
        group_id,
    };
    state
        .ws_registry
        .send_to_device(recipient_user_id, recipient_device_id, frame);
}

/// Уведомляет контакты об изменении ключа (добавление/отзыв устройства).
pub async fn notify_key_change(
    state: &AppState,
    contacts: &[Uuid],
    user_id: Uuid,
    device_id: Uuid,
    event: &str,
) {
    let frame = ServerFrame::KeyChange {
        user_id,
        device_id,
        event: event.to_string(),
    };

    for cu in contacts {
        state.ws_registry.send_to_user(*cu, &frame);
    }
}
