//! Endpoints для MLS Messaging: группы, Commit'ы, сообщения, welcome'ы, реакции.
//!
//! ## Endpoints
//!
//! | Method | Path | Описание |
//! |--------|------|----------|
//! | POST   | `/v1/groups` | Создание группы |
//! | POST   | `/v1/groups/create-direct` | Создание direct-чата (без MLS) |
//! | GET    | `/v1/groups/me` | Список моих групп |
//! | GET    | `/v1/groups/:id/members` | Участники группы |
//! | POST   | `/v1/groups/:id/commit` | Атомарный Commit (epoch bump) |
//! | GET    | `/v1/groups/:id/messages` | Pull сообщений + auto delivery |
//! | POST   | `/v1/groups/:id/messages` | Отправка application message |
//! | POST   | `/v1/messages/:id/state` | Edit/delete state |
//! | GET    | `/v1/messages/:id/delivery` | Статус доставки |
//! | GET    | `/v1/welcomes/me` | Невостребованные welcome'ы |
//! | POST   | `/v1/welcomes/:id/ack` | Подтверждение welcome |
//! | POST   | `/v1/messages/:id/reactions` | Добавить реакцию |
//! | DELETE | `/v1/messages/:id/reactions/:blind_index_hex` | Удалить реакцию |

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use messenger_entity::devices::Entity as Devices;
use messenger_entity::message_delivery_receipts;
use messenger_entity::mls_group_devices;
use messenger_entity::mls_group_members;
use messenger_entity::mls_groups;
use messenger_entity::mls_message_states;
use messenger_entity::mls_messages;
use messenger_entity::mls_welcomes;
use messenger_entity::reactions;
use messenger_entity::users::Entity as Users;
use sea_orm::{
    AccessMode, ActiveModelTrait, ColumnTrait, Condition, EntityTrait, IsolationLevel,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use sea_orm::sea_query::OnConflict;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use axum::response::IntoResponse;

use crate::attachments::StoredRef;
use crate::auth::middleware::CurrentAuth;
use crate::error::{decode_body, typed_response, AppError};
use crate::routes::ws::{notify_message_to_group, notify_welcome};
use crate::services::invite::now_secs;
use crate::state::AppState;

// ─── Request/Response types ───

/// Запрос на создание группы.
#[derive(Deserialize)]
pub struct CreateGroupRequest {
    /// Client-chosen group id. Must equal the MLS GroupId embedded in
    /// `initial_commit`/`welcomes` so the server's addressing matches every
    /// client's local MLS state. Rejected if it already exists.
    pub group_id: Uuid,
    pub group_type: String,
    #[serde(with = "serde_bytes")]
    pub initial_commit: Vec<u8>,
    pub welcomes: Vec<WelcomeForDevice>,
    pub member_devices: Vec<MemberDeviceInit>,
}

/// Welcome для одного устройства при создании/commit'е.
#[derive(Deserialize)]
pub struct WelcomeForDevice {
    pub recipient_device_id: Uuid,
    #[serde(with = "serde_bytes")]
    pub welcome_ciphertext: Vec<u8>,
}

/// Информация об устройстве-участнике при создании группы.
#[derive(Deserialize)]
pub struct MemberDeviceInit {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub leaf_index: i32,
    pub role_in_chat: String,
}

/// Ответ на создание группы.
#[derive(Serialize)]
pub struct CreateGroupResponse {
    pub group_id: Uuid,
    pub epoch: i64,
    pub created_at: i64,
}

/// Query для списка групп.
#[derive(Deserialize)]
pub struct ListMyGroupsQuery {
    pub since_id: Option<Uuid>,
    pub limit: Option<usize>,
}

/// Сводка одной группы.
#[derive(Serialize)]
pub struct GroupSummary {
    pub id: Uuid,
    pub group_type: String,
    pub current_epoch: i64,
    pub created_at: i64,
    pub role_in_chat: String,
    pub joined_at: i64,
}

/// Ответ на список групп.
#[derive(Serialize)]
pub struct ListMyGroupsResponse {
    pub groups: Vec<GroupSummary>,
    pub has_more: bool,
}

/// Один участник группы.
#[derive(Serialize)]
pub struct GroupMember {
    pub user_id: Uuid,
    pub role_in_chat: String,
    pub joined_at_epoch: i64,
    pub left_at_epoch: Option<i64>,
}

/// Одно устройство в группе.
#[derive(Serialize)]
pub struct GroupDevice {
    pub device_id: Uuid,
    pub user_id: Uuid,
    pub leaf_index: Option<i32>,
    pub added_at_epoch: i64,
    pub removed_at_epoch: Option<i64>,
}

/// Ответ на список участников.
#[derive(Serialize)]
pub struct GroupMembersResponse {
    pub members: Vec<GroupMember>,
    pub devices: Vec<GroupDevice>,
}

/// Запрос Commit'а.
#[derive(Deserialize)]
pub struct PostCommitRequest {
    pub expected_epoch: i64,
    #[serde(with = "serde_bytes")]
    pub commit: Vec<u8>,
    pub welcomes: Vec<WelcomeForDevice>,
    pub member_changes: Vec<MemberChange>,
}

/// Изменение участника в Commit'е.
#[derive(Deserialize)]
pub struct MemberChange {
    pub kind: String, // "add" | "remove"
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub leaf_index: Option<i32>,
    pub role_in_chat: Option<String>,
}

/// Ответ на Commit.
#[derive(Serialize)]
pub struct PostCommitResponse {
    pub message_id: Uuid,
    pub new_epoch: i64,
}

/// Query для pull сообщений.
#[derive(Deserialize)]
pub struct PullMessagesQuery {
    pub since_id: Option<Uuid>,
    pub limit: Option<usize>,
}

/// Состояние сообщения (edit/delete).
#[derive(Serialize)]
pub struct MessageState {
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub replacement_message_id: Option<Uuid>,
}

/// Одно сообщение при pull'е.
#[derive(Serialize)]
pub struct StoredMessage {
    pub id: Uuid,
    pub group_id: Uuid,
    pub epoch: i64,
    pub sender_user_id: Uuid,
    pub sender_device_id: Uuid,
    pub wire_format: String,
    #[serde(with = "serde_bytes")]
    pub mls_ciphertext: Vec<u8>,
    pub parent_message_id: Option<Uuid>,
    pub thread_root_id: Option<Uuid>,
    pub reply_to_message_id: Option<Uuid>,
    pub client_message_id: Uuid,
    pub created_at: i64,
    pub state: Option<MessageState>,
}

/// Ответ на pull сообщений.
#[derive(Serialize)]
pub struct PullMessagesResponse {
    pub messages: Vec<StoredMessage>,
    pub has_more: bool,
}

/// Запрос на отправку application message.
#[derive(Deserialize)]
pub struct PostMessageRequest {
    pub expected_epoch: i64,
    #[serde(with = "serde_bytes")]
    pub mls_ciphertext: Vec<u8>,
    pub parent_message_id: Option<Uuid>,
    pub reply_to_message_id: Option<Uuid>,
    pub thread_root_id: Option<Uuid>,
    pub client_message_id: Uuid,
}

/// Ответ на отправку сообщения.
#[derive(Serialize)]
pub struct PostMessageResponse {
    pub message_id: Uuid,
    pub created_at: i64,
}

/// Запрос на обновление состояния сообщения.
#[derive(Deserialize)]
pub struct UpdateMessageStateRequest {
    pub kind: String, // "edit" | "delete"
    pub replacement_message_id: Option<Uuid>,
}

/// Доставка одного устройства.
#[derive(Serialize)]
pub struct PerDeviceDelivery {
    pub device_id: Uuid,
    pub delivered_at: Option<i64>,
}

/// Статус доставки сообщения.
#[derive(Serialize)]
pub struct DeliveryStatus {
    pub message_id: Uuid,
    pub total_devices: i64,
    pub delivered_count: i64,
    pub per_device: Vec<PerDeviceDelivery>,
}

/// Query для списка welcome'ов.
#[derive(Deserialize)]
pub struct ListWelcomesQuery {
    pub since_id: Option<Uuid>,
}

/// Один pending welcome.
#[derive(Serialize)]
pub struct PendingWelcome {
    pub id: Uuid,
    pub group_id: Uuid,
    pub recipient_device_id: Uuid,
    pub epoch: i64,
    #[serde(with = "serde_bytes")]
    pub welcome_ciphertext: Vec<u8>,
    pub created_at: i64,
}

/// Ответ на список welcome'ов.
#[derive(Serialize)]
pub struct ListWelcomesResponse {
    pub welcomes: Vec<PendingWelcome>,
}

/// Запрос на добавление реакции.
#[derive(Deserialize)]
pub struct AddReactionRequest {
    #[serde(with = "serde_bytes")]
    pub reaction_blind_index: Vec<u8>,
    pub applied_at_epoch: i64,
}

// ─── Constants ───

/// Максимум сообщений при pull'е (по умолчанию). Включает служебные строки
/// (read-receipts, avatar-update), поэтому держим запас над числом видимых.
const DEFAULT_PULL_LIMIT: usize = 300;

/// Максимум сообщений при pull'е.
const MAX_PULL_LIMIT: usize = 1000;

/// Ciphersuite по умолчанию.
const MLS_CIPHERSUITE: i32 = 0x0001;

// ─── Handlers ───

/// Создание группы.
///
/// # Errors
///
/// - `400` — невалидный `group_type`, creator не в `member_devices`.
/// - `404` — referenced user/device не найден.
/// - `500` — ошибка БД.
#[allow(clippy::too_many_lines)]
pub async fn create_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: CreateGroupRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    // Валидация
    if req.group_type != "direct" && req.group_type != "group" {
        return Err(AppError::BadRequest("group_type must be 'direct' or 'group'".into()));
    }

    // Проверить что создатель — среди member_devices
    let creator_in_members = req.member_devices.iter().any(|md| {
        md.user_id == ctx.user.id && md.device_id == ctx.device.id
    });
    if !creator_in_members {
        return Err(AppError::BadRequest("creator must be in member_devices".into()));
    }

    // Валидация всех referenced user_id и device_id
    for md in &req.member_devices {
        let user = Users::find_by_id(md.user_id).one(&state.db).await?.ok_or_else(|| {
            AppError::BadRequest(format!("user {} not found", md.user_id))
        })?;
        if user.status != "active" {
            return Err(AppError::BadRequest(format!("user {} is not active", md.user_id)));
        }
        let device = Devices::find_by_id(md.device_id).one(&state.db).await?.ok_or_else(|| {
            AppError::BadRequest(format!("device {} not found", md.device_id))
        })?;
        if device.user_id != md.user_id {
            return Err(AppError::BadRequest(format!(
                "device {} does not belong to user {}",
                md.device_id, md.user_id
            )));
        }
        if device.revoked_at.is_some() {
            return Err(AppError::BadRequest(format!("device {} is revoked", md.device_id)));
        }
    }

    // Транзакция
    let txn = state
        .db
        .begin_with_config(Some(IsolationLevel::Serializable), Some(AccessMode::ReadWrite))
        .await?;

    // Use the client-chosen id so the server addresses the group by the same
    // value as the MLS GroupId. Reject collisions.
    let group_id = req.group_id;
    if mls_groups::Entity::find_by_id(group_id).one(&txn).await?.is_some() {
        return Err(AppError::BadRequest("group already exists".into()));
    }

    // 1. INSERT mls_groups
    mls_groups::ActiveModel {
        id: Set(group_id),
        group_type: Set(req.group_type.clone()),
        current_epoch: Set(0),
        ciphersuite: Set(MLS_CIPHERSUITE),
        created_at: Set(now),
        created_by_user_id: Set(ctx.user.id),
    }
    .insert(&txn)
    .await?;

    // 2. INSERT initial commit
    let commit_id = Uuid::now_v7();
    mls_messages::ActiveModel {
        id: Set(commit_id),
        group_id: Set(group_id),
        epoch: Set(0),
        sender_user_id: Set(ctx.user.id),
        sender_device_id: Set(ctx.device.id),
        wire_format: Set("proposal".into()),
        mls_ciphertext: Set(req.initial_commit.clone()),
        parent_message_id: Set(None),
        thread_root_id: Set(None),
        reply_to_message_id: Set(None),
        client_message_id: Set(Uuid::now_v7()),
        created_at: Set(now),
    }
    .insert(&txn)
    .await?;

    // 3. INSERT mls_group_members
    let mut seen_users: Vec<Uuid> = Vec::new();
    for md in &req.member_devices {
        if seen_users.contains(&md.user_id) {
            continue;
        }
        seen_users.push(md.user_id);

        let role = if md.user_id == ctx.user.id {
            "owner"
        } else {
            &md.role_in_chat
        };

        mls_group_members::ActiveModel {
            group_id: Set(group_id),
            user_id: Set(md.user_id),
            role_in_chat: Set(role.to_string()),
            joined_at_epoch: Set(0),
            left_at_epoch: Set(None),
            joined_at: Set(now),
        }
        .insert(&txn)
        .await?;
    }

    // 4. INSERT mls_group_devices
    for md in &req.member_devices {
        mls_group_devices::ActiveModel {
            group_id: Set(group_id),
            device_id: Set(md.device_id),
            leaf_index: Set(Some(md.leaf_index)),
            added_at_epoch: Set(0),
            removed_at_epoch: Set(None),
        }
        .insert(&txn)
        .await?;
    }

    // 5. INSERT welcomes — сохраняем ID для нотификаций
    let mut inserted_welcomes: Vec<(Uuid, Uuid, Uuid)> = Vec::new(); // (welcome_id, recipient_device_id, recipient_user_id)
    for w in &req.welcomes {
        let welcome_id = Uuid::now_v7();
        // Найти user_id владельца recipient_device_id
        let recipient_user_id = if let Ok(Some(dev)) = messenger_entity::devices::Entity::find_by_id(w.recipient_device_id)
            .one(&txn)
            .await
        {
            dev.user_id
        } else {
            continue;
        };

        mls_welcomes::ActiveModel {
            id: Set(welcome_id),
            group_id: Set(group_id),
            recipient_device_id: Set(w.recipient_device_id),
            epoch: Set(0),
            welcome_ciphertext: Set(w.welcome_ciphertext.clone()),
            created_at: Set(now),
            consumed_at: Set(None),
        }
        .insert(&txn)
        .await?;

        inserted_welcomes.push((welcome_id, w.recipient_device_id, recipient_user_id));
    }

    txn.commit().await?;

    // WS уведомления после commit
    let state_clone = state.clone();
    tokio::spawn(async move {
        for (welcome_id, recipient_device_id, recipient_user_id) in inserted_welcomes {
            notify_welcome(
                &state_clone,
                recipient_user_id,
                recipient_device_id,
                welcome_id,
                group_id,
            )
            .await;
        }
    });

    Ok(typed_response(
        &headers,
        StatusCode::CREATED,
        &CreateGroupResponse {
            group_id,
            epoch: 0,
            created_at: now,
        },
    ))
}

/// Запрос на создание direct-чата.
#[derive(Deserialize)]
pub struct CreateDirectChatRequest {
    pub target_username: String,
}

/// Создание direct-чата (серверная сторона, без MLS).
///
/// Находит пользователя по username, создаёт группу `group_type="direct"`
/// и добавляет в неё обоих пользователей со всеми их устройствами.
///
/// # Errors
///
/// - `400` — пустой username, попытка создать чат с самим собой.
/// - `404` — target пользователь не найден или не active.
/// - `500` — ошибка БД.
#[allow(clippy::cast_possible_wrap)]
/// Удаление группы (чата) вместе со всеми сообщениями и вложениями.
///
/// Authorization: запрашивающий должен быть участником группы.
///
/// Удаление строки `mls_groups` каскадит (ON DELETE CASCADE) на mls_messages,
/// members, devices, welcomes и — через messages — на reactions, receipts,
/// message_states. Вложения висят на message_id с ON DELETE SET NULL, поэтому
/// их файлы и строки удаляются явно (иначе блобы осиротеют на диске).
///
/// После удаления `create_direct_chat` с тем же собеседником больше не найдёт
/// общую активную группу и создаст новую пустую.
///
/// # Errors
///
/// - `404` — группа не найдена.
/// - `403` — запрашивающий не участник группы.
/// - `500` — ошибка БД или ввода-вывода.
pub async fn delete_group(
    State(state): State<AppState>,
    CurrentAuth(ctx): CurrentAuth,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    // 1. Группа существует.
    mls_groups::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // 2. Запрашивающий — участник (активный или вышедший).
    let membership = mls_group_members::Entity::find()
        .filter(mls_group_members::Column::GroupId.eq(id))
        .filter(mls_group_members::Column::UserId.eq(ctx.user.id))
        .one(&state.db)
        .await?;
    if membership.is_none() {
        return Err(AppError::GroupMembershipRequired);
    }

    // 3. Явно удалить вложения группы (файлы + строки): attachments → messages
    //    идёт через SET NULL, каскад их не заберёт.
    let messages = mls_messages::Entity::find()
        .filter(mls_messages::Column::GroupId.eq(id))
        .all(&state.db)
        .await?;
    let message_ids: Vec<Uuid> = messages.iter().map(|m| m.id).collect();
    if !message_ids.is_empty() {
        let atts = messenger_entity::attachments::Entity::find()
            .filter(messenger_entity::attachments::Column::MessageId.is_in(message_ids))
            .all(&state.db)
            .await?;
        for a in &atts {
            if let Some(ref sref_str) = a.storage_ref {
                #[allow(clippy::cast_sign_loss)]
                let sref = StoredRef::OnDisk {
                    relative_path: std::path::PathBuf::from(sref_str),
                    size: a.padded_size as u64,
                };
                if let Err(e) = state.storage.delete(&sref).await {
                    tracing::warn!(
                        attachment_id = %a.id,
                        error = ?e,
                        "delete_group: failed to delete attachment file"
                    );
                }
            }
            messenger_entity::attachments::Entity::delete_by_id(a.id)
                .exec(&state.db)
                .await?;
        }
    }

    // 4. Удалить группу — каскад заберёт всё остальное.
    mls_groups::Entity::delete_by_id(id).exec(&state.db).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Запрос передачи прав владельца группы.
#[derive(Deserialize)]
pub struct TransferOwnerRequest {
    pub new_owner_user_id: Uuid,
}

/// Передать роль `owner` другому активному участнику.
///
/// Вызвать может только текущий владелец. Преемник должен быть активным
/// участником. Старый владелец становится `member`. Роли — это app-level
/// метаданные (не MLS-операция), поэтому это отдельный эндпоинт, а не commit.
///
/// # Errors
///
/// - `403` — вызывающий не владелец.
/// - `400` — преемник не активный участник.
pub async fn transfer_owner(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(group_id): Path<Uuid>,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: TransferOwnerRequest = decode_body(&headers, &body)?;

    let txn = state
        .db
        .begin_with_config(Some(IsolationLevel::Serializable), Some(AccessMode::ReadWrite))
        .await?;

    // Вызывающий — активный владелец?
    let me = mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.eq(group_id))
                .add(mls_group_members::Column::UserId.eq(ctx.user.id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&txn)
        .await?
        .ok_or(AppError::GroupMembershipRequired)?;
    if me.role_in_chat != "owner" {
        return Err(AppError::Forbidden);
    }

    // Передача самому себе — no-op.
    if req.new_owner_user_id == ctx.user.id {
        txn.commit().await?;
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    // Преемник — активный участник?
    let successor = mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.eq(group_id))
                .add(mls_group_members::Column::UserId.eq(req.new_owner_user_id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::BadRequest("new owner is not an active member".into()))?;

    let mut succ: mls_group_members::ActiveModel = successor.into();
    succ.role_in_chat = Set("owner".into());
    succ.update(&txn).await?;

    let mut old: mls_group_members::ActiveModel = me.into();
    old.role_in_chat = Set("member".into());
    old.update(&txn).await?;

    txn.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn create_direct_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: CreateDirectChatRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    if req.target_username.trim().is_empty() {
        return Err(AppError::BadRequest("target_username cannot be empty".into()));
    }

    // Найти target пользователя по blind_index
    let blind_index = state.server_identity.blind_index(&req.target_username);
    let target_user = messenger_entity::users::Entity::find()
        .filter(messenger_entity::users::Column::UsernameBlindIndex.eq(blind_index))
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if target_user.status != "active" {
        return Err(AppError::NotFound);
    }

    if target_user.id == ctx.user.id {
        return Err(AppError::BadRequest("cannot create direct chat with yourself".into()));
    }

    // Дедупликация: direct-чат между парой пользователей может быть только
    // один. Если он уже существует — возвращаем его вместо создания дубля.
    {
        let my_groups: Vec<Uuid> = mls_group_members::Entity::find()
            .filter(mls_group_members::Column::UserId.eq(ctx.user.id))
            .filter(mls_group_members::Column::LeftAtEpoch.is_null())
            .all(&state.db)
            .await?
            .into_iter()
            .map(|m| m.group_id)
            .collect();
        if !my_groups.is_empty() {
            let shared = mls_group_members::Entity::find()
                .filter(mls_group_members::Column::UserId.eq(target_user.id))
                .filter(mls_group_members::Column::LeftAtEpoch.is_null())
                .filter(mls_group_members::Column::GroupId.is_in(my_groups))
                .all(&state.db)
                .await?;
            for membership in shared {
                let group = mls_groups::Entity::find_by_id(membership.group_id)
                    .one(&state.db)
                    .await?;
                if let Some(group) = group.filter(|g| g.group_type == "direct") {
                    return Ok(typed_response(
                        &headers,
                        StatusCode::OK,
                        &CreateGroupResponse {
                            group_id: group.id,
                            epoch: group.current_epoch,
                            created_at: group.created_at,
                        },
                    ));
                }
            }
        }
    }

    // Найти все активные устройства creator'а и target'а
    let creator_devices = messenger_entity::devices::Entity::find()
        .filter(messenger_entity::devices::Column::UserId.eq(ctx.user.id))
        .filter(messenger_entity::devices::Column::RevokedAt.is_null())
        .all(&state.db)
        .await?;

    let target_devices = messenger_entity::devices::Entity::find()
        .filter(messenger_entity::devices::Column::UserId.eq(target_user.id))
        .filter(messenger_entity::devices::Column::RevokedAt.is_null())
        .all(&state.db)
        .await?;

    let all_devices: Vec<&messenger_entity::devices::Model> = creator_devices
        .iter()
        .chain(target_devices.iter())
        .collect();

    if all_devices.is_empty() {
        return Err(AppError::BadRequest("no active devices available".into()));
    }

    // Транзакция
    let txn = state
        .db
        .begin_with_config(Some(IsolationLevel::Serializable), Some(AccessMode::ReadWrite))
        .await?;

    let group_id = Uuid::now_v7();

    // 1. INSERT mls_groups
    mls_groups::ActiveModel {
        id: Set(group_id),
        group_type: Set("direct".into()),
        current_epoch: Set(0),
        ciphersuite: Set(MLS_CIPHERSUITE),
        created_at: Set(now),
        created_by_user_id: Set(ctx.user.id),
    }
    .insert(&txn)
    .await?;

    // 2. INSERT mls_group_members
    mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(ctx.user.id),
        role_in_chat: Set("owner".into()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now),
    }
    .insert(&txn)
    .await?;

    mls_group_members::ActiveModel {
        group_id: Set(group_id),
        user_id: Set(target_user.id),
        role_in_chat: Set("member".into()),
        joined_at_epoch: Set(0),
        left_at_epoch: Set(None),
        joined_at: Set(now),
    }
    .insert(&txn)
    .await?;

    // 3. INSERT mls_group_devices (leaf_index: 0, 1, 2, ...)
    for (idx, device) in all_devices.iter().enumerate() {
        let leaf_index = i32::try_from(idx).unwrap_or(0);
        mls_group_devices::ActiveModel {
            group_id: Set(group_id),
            device_id: Set(device.id),
            leaf_index: Set(Some(leaf_index)),
            added_at_epoch: Set(0),
            removed_at_epoch: Set(None),
        }
        .insert(&txn)
        .await?;
    }

    // 4. INSERT placeholder initial message (wire_format="proposal")
    mls_messages::ActiveModel {
        id: Set(Uuid::now_v7()),
        group_id: Set(group_id),
        epoch: Set(0),
        sender_user_id: Set(ctx.user.id),
        sender_device_id: Set(ctx.device.id),
        wire_format: Set("proposal".into()),
        mls_ciphertext: Set(Vec::new()),
        parent_message_id: Set(None),
        thread_root_id: Set(None),
        reply_to_message_id: Set(None),
        client_message_id: Set(Uuid::now_v7()),
        created_at: Set(now),
    }
    .insert(&txn)
    .await?;

    txn.commit().await?;

    Ok(typed_response(
        &headers,
        StatusCode::CREATED,
        &CreateGroupResponse {
            group_id,
            epoch: 0,
            created_at: now,
        },
    ))
}

/// Список моих групп.
///
/// # Errors
///
/// - `500` — ошибка БД.
#[allow(clippy::cast_possible_wrap)]
pub async fn list_my_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Query(query): Query<ListMyGroupsQuery>,
) -> Result<Response, AppError> {
    let limit = query.limit.unwrap_or(50).min(100);

    let mut cond = Condition::all()
        .add(mls_group_members::Column::UserId.eq(ctx.user.id))
        .add(mls_group_members::Column::LeftAtEpoch.is_null());

    if let Some(since_id) = query.since_id {
        cond = cond.add(mls_groups::Column::Id.gt(since_id));
    }

    let memberships = mls_group_members::Entity::find()
        .filter(cond)
        .order_by_asc(mls_group_members::Column::GroupId)
        .limit(Some((limit + 1) as u64))
        .all(&state.db)
        .await?;

    let has_more = memberships.len() > limit;
    let memberships: Vec<_> = memberships.into_iter().take(limit).collect();

    let mut groups = Vec::with_capacity(memberships.len());
    for membership in memberships {
        if let Some(group) = mls_groups::Entity::find_by_id(membership.group_id)
            .one(&state.db)
            .await?
        {
            groups.push(GroupSummary {
                id: group.id,
                group_type: group.group_type,
                current_epoch: group.current_epoch,
                created_at: group.created_at,
                role_in_chat: membership.role_in_chat,
                joined_at: membership.joined_at,
            });
        }
    }

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &ListMyGroupsResponse { groups, has_more },
    ))
}

/// Участники группы.
///
/// # Errors
///
/// - `403` — текущий пользователь не участник группы.
/// - `404` — группа не найдена.
/// - `500` — ошибка БД.
pub async fn get_group_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(group_id): Path<Uuid>,
) -> Result<Response, AppError> {
    // Проверить что группа существует
    let _group = mls_groups::Entity::find_by_id(group_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Проверить membership
    let is_member = mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.eq(group_id))
                .add(mls_group_members::Column::UserId.eq(ctx.user.id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&state.db)
        .await?
        .is_some();

    if !is_member {
        return Err(AppError::GroupMembershipRequired);
    }

    // Members
    let members = mls_group_members::Entity::find()
        .filter(mls_group_members::Column::GroupId.eq(group_id))
        .all(&state.db)
        .await?
        .into_iter()
        .map(|m| GroupMember {
            user_id: m.user_id,
            role_in_chat: m.role_in_chat,
            joined_at_epoch: m.joined_at_epoch,
            left_at_epoch: m.left_at_epoch,
        })
        .collect();

    // Devices with user_id lookup
    let group_devices = mls_group_devices::Entity::find()
        .filter(mls_group_devices::Column::GroupId.eq(group_id))
        .all(&state.db)
        .await?;

    let mut devices = Vec::with_capacity(group_devices.len());
    for gd in group_devices {
        let device = Devices::find_by_id(gd.device_id).one(&state.db).await?;
        devices.push(GroupDevice {
            device_id: gd.device_id,
            user_id: device.map_or(Uuid::default(), |d| d.user_id),
            leaf_index: gd.leaf_index,
            added_at_epoch: gd.added_at_epoch,
            removed_at_epoch: gd.removed_at_epoch,
        });
    }

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &GroupMembersResponse { members, devices },
    ))
}

/// Атомарный Commit — самый сложный endpoint.
///
/// Проверяет epoch, авторизует `member_changes`, вставляет Commit и Welcome'ы,
/// обновляет epoch и member/devices.
///
/// # Errors
///
/// - `403` — не участник группы или недостаточно прав.
/// - `409 ERR_EPOCH_OUTDATED` — epoch не совпадает или commit уже есть.
/// - `400` — unknown change kind.
/// - `500` — ошибка БД.
#[allow(clippy::too_many_lines)]
pub async fn post_commit(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(group_id): Path<Uuid>,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: PostCommitRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    let txn = state
        .db
        .begin_with_config(Some(IsolationLevel::Serializable), Some(AccessMode::ReadWrite))
        .await?;

    // 1. Проверить membership
    let membership = mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.eq(group_id))
                .add(mls_group_members::Column::UserId.eq(ctx.user.id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&txn)
        .await?
        .ok_or(AppError::GroupMembershipRequired)?;

    // 2. Проверить epoch
    let group = mls_groups::Entity::find_by_id(group_id)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound)?;

    if group.current_epoch != req.expected_epoch {
        return Err(AppError::EpochOutdated);
    }

    // 3. Проверить что в этом epoch ещё нет commit'а
    let existing_commit = mls_messages::Entity::find()
        .filter(
            Condition::all()
                .add(mls_messages::Column::GroupId.eq(group_id))
                .add(mls_messages::Column::Epoch.eq(req.expected_epoch))
                .add(mls_messages::Column::WireFormat.eq("commit")),
        )
        .one(&txn)
        .await?;

    if existing_commit.is_some() {
        return Err(AppError::EpochOutdated);
    }

    // 4. Авторизация member_changes
    for change in &req.member_changes {
        match change.kind.as_str() {
            "add" => {
                let is_self_device = change.user_id == ctx.user.id;
                let is_privileged =
                    matches!(membership.role_in_chat.as_str(), "owner" | "admin");
                // Adding a device whose user is ALREADY an active member is a
                // device-add of an existing participant (e.g. healing a member's
                // re-provisioned device), NOT adding a new member — so any active
                // member may do it, not just the owner. This lets a co-member
                // (e.g. the peer in a direct chat) pull a re-logged device back
                // into the group without the chat owner being online. The device
                // is still verified to belong to that user in step 8.
                let adds_existing_member_device = !is_self_device
                    && !is_privileged
                    && mls_group_members::Entity::find()
                        .filter(
                            Condition::all()
                                .add(mls_group_members::Column::GroupId.eq(group_id))
                                .add(mls_group_members::Column::UserId.eq(change.user_id))
                                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
                        )
                        .one(&txn)
                        .await?
                        .is_some();
                if !is_self_device && !is_privileged && !adds_existing_member_device {
                    return Err(AppError::Forbidden);
                }
            }
            "remove" => {
                let is_self_device = change.user_id == ctx.user.id;
                let is_privileged =
                    matches!(membership.role_in_chat.as_str(), "owner" | "admin");
                if !is_self_device && !is_privileged {
                    return Err(AppError::Forbidden);
                }
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "unknown change kind: {}",
                    change.kind
                )));
            }
        }
    }

    let new_epoch = group.current_epoch + 1;
    let message_id = Uuid::now_v7();

    // 5. INSERT commit
    mls_messages::ActiveModel {
        id: Set(message_id),
        group_id: Set(group_id),
        epoch: Set(req.expected_epoch),
        sender_user_id: Set(ctx.user.id),
        sender_device_id: Set(ctx.device.id),
        wire_format: Set("commit".into()),
        mls_ciphertext: Set(req.commit.clone()),
        parent_message_id: Set(None),
        thread_root_id: Set(None),
        reply_to_message_id: Set(None),
        client_message_id: Set(Uuid::now_v7()),
        created_at: Set(now),
    }
    .insert(&txn)
    .await?;

    // 6. UPDATE epoch
    let mut group_active: mls_groups::ActiveModel = group.into();
    group_active.current_epoch = Set(new_epoch);
    group_active.update(&txn).await?;

    // 7. INSERT welcomes — сохраняем ID для нотификаций
    let mut inserted_welcomes: Vec<(Uuid, Uuid, Uuid)> = Vec::new();
    for w in &req.welcomes {
        let welcome_id = Uuid::now_v7();
        // Найти user_id владельца recipient_device_id
        let recipient_user_id = if let Ok(Some(dev)) = messenger_entity::devices::Entity::find_by_id(w.recipient_device_id)
            .one(&txn)
            .await
        {
            dev.user_id
        } else {
            continue;
        };

        mls_welcomes::ActiveModel {
            id: Set(welcome_id),
            group_id: Set(group_id),
            recipient_device_id: Set(w.recipient_device_id),
            epoch: Set(new_epoch),
            welcome_ciphertext: Set(w.welcome_ciphertext.clone()),
            consumed_at: Set(None),
            created_at: Set(now),
        }
        .insert(&txn)
        .await?;

        inserted_welcomes.push((welcome_id, w.recipient_device_id, recipient_user_id));
    }

    // 8. Применить member_changes
    for change in &req.member_changes {
        match change.kind.as_str() {
            "add" => {
                // Проверить что target user/device существуют и активны
                let target_user = Users::find_by_id(change.user_id)
                    .one(&txn)
                    .await?
                    .ok_or_else(|| {
                        AppError::BadRequest(format!("user {} not found", change.user_id))
                    })?;
                if target_user.status != "active" {
                    return Err(AppError::BadRequest(format!(
                        "user {} is not active",
                        change.user_id
                    )));
                }
                let target_device = Devices::find_by_id(change.device_id)
                    .one(&txn)
                    .await?
                    .ok_or_else(|| {
                        AppError::BadRequest(format!("device {} not found", change.device_id))
                    })?;
                if target_device.user_id != change.user_id {
                    return Err(AppError::BadRequest(format!(
                        "device {} does not belong to user {}",
                        change.device_id, change.user_id
                    )));
                }
                if target_device.revoked_at.is_some() {
                    return Err(AppError::BadRequest(format!(
                        "device {} is revoked",
                        change.device_id
                    )));
                }

                // Добавить member если ещё не
                let existing_member = mls_group_members::Entity::find()
                    .filter(
                        Condition::all()
                            .add(mls_group_members::Column::GroupId.eq(group_id))
                            .add(mls_group_members::Column::UserId.eq(change.user_id))
                            .add(mls_group_members::Column::LeftAtEpoch.is_null()),
                    )
                    .one(&txn)
                    .await?;

                if existing_member.is_none() {
                    mls_group_members::ActiveModel {
                        group_id: Set(group_id),
                        user_id: Set(change.user_id),
                        role_in_chat: Set(change.role_in_chat.clone().unwrap_or("member".into())),
                        joined_at_epoch: Set(new_epoch),
                        left_at_epoch: Set(None),
                        joined_at: Set(now),
                    }
                    .insert(&txn)
                    .await?;
                }

                // Добавить device если ещё нет
                let existing_gd = mls_group_devices::Entity::find()
                    .filter(
                        Condition::all()
                            .add(mls_group_devices::Column::GroupId.eq(group_id))
                            .add(mls_group_devices::Column::DeviceId.eq(change.device_id))
                            .add(mls_group_devices::Column::RemovedAtEpoch.is_null()),
                    )
                    .one(&txn)
                    .await?;

                if existing_gd.is_none() {
                    mls_group_devices::ActiveModel {
                        group_id: Set(group_id),
                        device_id: Set(change.device_id),
                        leaf_index: Set(change.leaf_index),
                        added_at_epoch: Set(new_epoch),
                        removed_at_epoch: Set(None),
                    }
                    .insert(&txn)
                    .await?;
                }
            }
            "remove" => {
                // Пометить device как removed
                let gd = mls_group_devices::Entity::find()
                    .filter(
                        Condition::all()
                            .add(mls_group_devices::Column::GroupId.eq(group_id))
                            .add(mls_group_devices::Column::DeviceId.eq(change.device_id))
                            .add(mls_group_devices::Column::RemovedAtEpoch.is_null()),
                    )
                    .one(&txn)
                    .await?;

                if let Some(gd) = gd {
                    let mut active: mls_group_devices::ActiveModel = gd.into();
                    active.removed_at_epoch = Set(Some(new_epoch));
                    active.update(&txn).await?;
                }

                // Проверить, остались ли у user'а активные devices в группе
                let user_id = change.user_id;
                let remaining_devices = mls_group_devices::Entity::find()
                    .filter(
                        Condition::all()
                            .add(mls_group_devices::Column::GroupId.eq(group_id))
                            .add(mls_group_devices::Column::RemovedAtEpoch.is_null()),
                    )
                    .all(&txn)
                    .await?;

                // Если у user'а нет больше активных devices, пометить left
                // Загружаем все активные devices этого user'а в группе
                let mut user_has_device = false;
                for gd in &remaining_devices {
                    if let Some(dev) = Devices::find_by_id(gd.device_id).one(&txn).await? {
                        if dev.user_id == user_id {
                            user_has_device = true;
                            break;
                        }
                    }
                }

                if !user_has_device {
                    // Пометить member как left
                    let member = mls_group_members::Entity::find()
                        .filter(
                            Condition::all()
                                .add(mls_group_members::Column::GroupId.eq(group_id))
                                .add(mls_group_members::Column::UserId.eq(user_id))
                                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
                        )
                        .one(&txn)
                        .await?;

                    if let Some(member) = member {
                        let mut active: mls_group_members::ActiveModel = member.into();
                        active.left_at_epoch = Set(Some(new_epoch));
                        active.update(&txn).await?;
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    txn.commit().await?;

    // WS уведомления после commit (в background, не блокируем ответ)
    let state_clone = state.clone();
    let gid = group_id;
    let mid = message_id;
    let nep = new_epoch;
    tokio::spawn(async move {
        notify_message_to_group(&state_clone, gid, mid, nep).await;
        for (welcome_id, recipient_device_id, recipient_user_id) in inserted_welcomes {
            notify_welcome(
                &state_clone,
                recipient_user_id,
                recipient_device_id,
                welcome_id,
                gid,
            )
            .await;
        }
    });

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &PostCommitResponse {
            message_id,
            new_epoch,
        },
    ))
}

/// Pull сообщений с автоматическим проставлением delivery receipts.
///
/// # Errors
///
/// - `403` — не участник группы.
/// - `500` — ошибка БД.
#[allow(clippy::too_many_lines, clippy::cast_possible_wrap)]
pub async fn pull_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(group_id): Path<Uuid>,
    Query(query): Query<PullMessagesQuery>,
) -> Result<Response, AppError> {
    // Проверить membership
    let is_member = mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.eq(group_id))
                .add(mls_group_members::Column::UserId.eq(ctx.user.id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&state.db)
        .await?
        .is_some();

    if !is_member {
        return Err(AppError::GroupMembershipRequired);
    }

    let limit = query
        .limit
        .unwrap_or(DEFAULT_PULL_LIMIT)
        .min(MAX_PULL_LIMIT);

    let mut cond = Condition::all().add(mls_messages::Column::GroupId.eq(group_id));
    if let Some(since_id) = query.since_id {
        cond = cond.add(mls_messages::Column::Id.gt(since_id));
    }

    // With a `since_id` cursor this is an incremental pull → messages strictly
    // after the cursor, oldest-first. Without one it's a full load → return the
    // LATEST `limit` messages (newest-first then reversed to oldest-first). The
    // old code always returned the OLDEST `limit`, so once a chat exceeded the
    // window the newest messages — including ones just sent — were never
    // fetched and the chat appeared dead. (`mls_messages` also holds control
    // rows — read receipts, avatar updates — so the window fills faster than the
    // visible message count suggests.)
    let (messages, has_more) = if query.since_id.is_some() {
        let rows = mls_messages::Entity::find()
            .filter(cond)
            .order_by_asc(mls_messages::Column::Id)
            .limit(Some((limit + 1) as u64))
            .all(&state.db)
            .await?;
        let has_more = rows.len() > limit;
        (rows.into_iter().take(limit).collect::<Vec<_>>(), has_more)
    } else {
        let mut rows = mls_messages::Entity::find()
            .filter(cond)
            .order_by_desc(mls_messages::Column::Id)
            .limit(Some((limit + 1) as u64))
            .all(&state.db)
            .await?;
        let has_more = rows.len() > limit;
        rows.truncate(limit); // keep the newest `limit`
        rows.reverse(); // oldest-first, as clients expect
        (rows, has_more)
    };

    // Bulk insert delivery receipts
    if !messages.is_empty() {
        let now = now_secs();
        let inserts: Vec<message_delivery_receipts::ActiveModel> = messages
            .iter()
            .map(|m| message_delivery_receipts::ActiveModel {
                message_id: Set(m.id),
                recipient_device_id: Set(ctx.device.id),
                delivered_at: Set(now),
            })
            .collect();

        let _ = message_delivery_receipts::Entity::insert_many(inserts)
            .on_conflict(
                OnConflict::columns([
                    message_delivery_receipts::Column::MessageId,
                    message_delivery_receipts::Column::RecipientDeviceId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&state.db)
            .await;
    }

    // Загрузить states для сообщений
    let message_ids: Vec<Uuid> = messages.iter().map(|m| m.id).collect();
    let states = if message_ids.is_empty() {
        vec![]
    } else {
        mls_message_states::Entity::find()
            .filter(mls_message_states::Column::MessageId.is_in(message_ids))
            .all(&state.db)
            .await?
    };

    let state_map: std::collections::HashMap<Uuid, mls_message_states::Model> =
        states.into_iter().map(|s| (s.message_id, s)).collect();

    let stored_messages: Vec<StoredMessage> = messages
        .into_iter()
        .map(|m| {
            let msg_state = state_map.get(&m.id).map(|s| MessageState {
                edited_at: s.edited_at,
                deleted_at: s.deleted_at,
                replacement_message_id: s.replacement_message_id,
            });
            StoredMessage {
                id: m.id,
                group_id: m.group_id,
                epoch: m.epoch,
                sender_user_id: m.sender_user_id,
                sender_device_id: m.sender_device_id,
                wire_format: m.wire_format,
                mls_ciphertext: m.mls_ciphertext,
                parent_message_id: m.parent_message_id,
                thread_root_id: m.thread_root_id,
                reply_to_message_id: m.reply_to_message_id,
                client_message_id: m.client_message_id,
                created_at: m.created_at,
                state: msg_state,
            }
        })
        .collect();

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &PullMessagesResponse {
            messages: stored_messages,
            has_more,
        },
    ))
}

/// Отправка application message.
///
/// # Errors
///
/// - `403` — не участник группы.
/// - `409` — duplicate (`sender_device_id`, `client_message_id`).
/// - `500` — ошибка БД.
#[allow(clippy::too_many_lines)]
pub async fn post_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(group_id): Path<Uuid>,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: PostMessageRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    // Membership check
    let is_member = mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.eq(group_id))
                .add(mls_group_members::Column::UserId.eq(ctx.user.id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&state.db)
        .await?
        .is_some();

    if !is_member {
        return Err(AppError::GroupMembershipRequired);
    }

    // Zero-knowledge-safe suspend enforcement: for a DIRECT (1:1) chat, refuse
    // to post if the counterpart account is suspended. This uses only group
    // membership and account status — both already known to the server — and
    // never inspects message content, so it preserves zero-knowledge.
    if let Some(group) = mls_groups::Entity::find_by_id(group_id).one(&state.db).await? {
        if group.group_type == "direct" {
            let other = mls_group_members::Entity::find()
                .filter(
                    Condition::all()
                        .add(mls_group_members::Column::GroupId.eq(group_id))
                        .add(mls_group_members::Column::UserId.ne(ctx.user.id))
                        .add(mls_group_members::Column::LeftAtEpoch.is_null()),
                )
                .one(&state.db)
                .await?;
            if let Some(member) = other {
                let suspended = Users::find_by_id(member.user_id)
                    .one(&state.db)
                    .await?
                    .map_or(false, |u| u.status == "suspended");
                if suspended {
                    return Err(AppError::RecipientSuspended);
                }
            }
        }
    }

    // Idempotency: check if (sender_device_id, client_message_id) already exists
    let existing = mls_messages::Entity::find()
        .filter(
            Condition::all()
                .add(mls_messages::Column::SenderDeviceId.eq(ctx.device.id))
                .add(mls_messages::Column::ClientMessageId.eq(req.client_message_id)),
        )
        .one(&state.db)
        .await?;

    if let Some(existing) = existing {
        return Ok(typed_response(
            &headers,
            StatusCode::OK,
            &PostMessageResponse {
                message_id: existing.id,
                created_at: existing.created_at,
            },
        ));
    }

    let message_id = Uuid::now_v7();

    mls_messages::ActiveModel {
        id: Set(message_id),
        group_id: Set(group_id),
        epoch: Set(req.expected_epoch),
        sender_user_id: Set(ctx.user.id),
        sender_device_id: Set(ctx.device.id),
        wire_format: Set("application".into()),
        mls_ciphertext: Set(req.mls_ciphertext),
        parent_message_id: Set(req.parent_message_id),
        thread_root_id: Set(req.thread_root_id),
        reply_to_message_id: Set(req.reply_to_message_id),
        client_message_id: Set(req.client_message_id),
        created_at: Set(now),
    }
    .insert(&state.db)
    .await?;

    // WS уведомление о новом сообщении (в background)
    let state_clone = state.clone();
    tokio::spawn(async move {
        notify_message_to_group(&state_clone, group_id, message_id, req.expected_epoch).await;
    });

    Ok(typed_response(
        &headers,
        StatusCode::CREATED,
        &PostMessageResponse {
            message_id,
            created_at: now,
        },
    ))
}

/// Маркировка edit/delete для сообщения.
///
/// # Errors
///
/// - `404` — сообщение не найдено.
/// - `403` — не автор сообщения.
/// - `400` — невалидный kind.
/// - `500` — ошибка БД.
pub async fn update_message_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(message_id): Path<Uuid>,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: UpdateMessageStateRequest = decode_body(&headers, &body)?;

    // Найти сообщение
    let message = mls_messages::Entity::find_by_id(message_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Проверить что sender == ctx.user
    if message.sender_user_id != ctx.user.id {
        return Err(AppError::Forbidden);
    }

    match req.kind.as_str() {
        "edit" => {
            let replacement_id = req.replacement_message_id.ok_or_else(|| {
                AppError::BadRequest("replacement_message_id required for edit".into())
            })?;

            // Проверить что replacement message существует в той же группе и от того же sender
            let replacement = mls_messages::Entity::find_by_id(replacement_id)
                .one(&state.db)
                .await?
                .ok_or_else(|| {
                    AppError::BadRequest("replacement message not found".into())
                })?;

            if replacement.group_id != message.group_id {
                return Err(AppError::BadRequest(
                    "replacement message must be in the same group".into(),
                ));
            }
            if replacement.sender_user_id != ctx.user.id {
                return Err(AppError::Forbidden);
            }

            // Upsert state
            let existing = mls_message_states::Entity::find_by_id(message_id)
                .one(&state.db)
                .await?;

            if let Some(existing) = existing {
                let mut active: mls_message_states::ActiveModel = existing.into();
                active.edited_at = Set(Some(now_secs()));
                active.replacement_message_id = Set(Some(replacement_id));
                active.update(&state.db).await?;
            } else {
                mls_message_states::ActiveModel {
                    message_id: Set(message_id),
                    edited_at: Set(Some(now_secs())),
                    deleted_at: Set(None),
                    replacement_message_id: Set(Some(replacement_id)),
                }
                .insert(&state.db)
                .await?;
            }
        }
        "delete" => {
            let existing = mls_message_states::Entity::find_by_id(message_id)
                .one(&state.db)
                .await?;

            if let Some(existing) = existing {
                let mut active: mls_message_states::ActiveModel = existing.into();
                active.deleted_at = Set(Some(now_secs()));
                active.update(&state.db).await?;
            } else {
                mls_message_states::ActiveModel {
                    message_id: Set(message_id),
                    edited_at: Set(None),
                    deleted_at: Set(Some(now_secs())),
                    replacement_message_id: Set(None),
                }
                .insert(&state.db)
                .await?;
            }

            // Free any attachment blobs owned by this message. The message keeps
            // its soft-delete tombstone, but the (now hidden) media must not
            // linger on disk — otherwise deleted photos/videos pile up forever,
            // since the GC only reclaims never-finalized uploads (message_id IS
            // NULL). Best-effort: log and continue on failure.
            let atts = messenger_entity::attachments::Entity::find()
                .filter(messenger_entity::attachments::Column::MessageId.eq(message_id))
                .all(&state.db)
                .await
                .unwrap_or_default();
            for a in &atts {
                if let Some(ref sref_str) = a.storage_ref {
                    #[allow(clippy::cast_sign_loss)]
                    let sref = crate::attachments::StoredRef::OnDisk {
                        relative_path: std::path::PathBuf::from(sref_str),
                        size: a.padded_size as u64,
                    };
                    if let Err(e) = state.storage.delete(&sref).await {
                        tracing::warn!(attachment_id = %a.id, error = ?e, "delete: blob removal failed");
                    }
                }
                if let Err(e) =
                    messenger_entity::attachments::Entity::delete_by_id(a.id)
                        .exec(&state.db)
                        .await
                {
                    tracing::warn!(attachment_id = %a.id, error = ?e, "delete: attachment row removal failed");
                }
            }
        }
        _ => {
            return Err(AppError::BadRequest("kind must be 'edit' or 'delete'".into()));
        }
    }

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Статус доставки сообщения.
///
/// # Errors
///
/// - `404` — сообщение не найдено.
/// - `403` — не автор сообщения.
/// - `500` — ошибка БД.
#[allow(clippy::cast_possible_wrap)]
pub async fn get_delivery_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(message_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let message = mls_messages::Entity::find_by_id(message_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Только sender может видеть статус доставки
    if message.sender_user_id != ctx.user.id {
        return Err(AppError::Forbidden);
    }

    // Активные devices в группе на момент отправки
    let active_devices = mls_group_devices::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_devices::Column::GroupId.eq(message.group_id))
                .add(mls_group_devices::Column::AddedAtEpoch.lte(message.epoch))
                .add(
                    Condition::any()
                        .add(mls_group_devices::Column::RemovedAtEpoch.is_null())
                        .add(mls_group_devices::Column::RemovedAtEpoch.gt(message.epoch)),
                ),
        )
        .all(&state.db)
        .await?;

    let total_devices = active_devices.len() as i64;

    // Получить delivery receipts для этого сообщения
    let receipts = message_delivery_receipts::Entity::find()
        .filter(message_delivery_receipts::Column::MessageId.eq(message_id))
        .all(&state.db)
        .await?;

    let receipt_map: std::collections::HashMap<Uuid, i64> = receipts
        .into_iter()
        .map(|r| (r.recipient_device_id, r.delivered_at))
        .collect();

    let per_device: Vec<PerDeviceDelivery> = active_devices
        .into_iter()
        .map(|gd| PerDeviceDelivery {
            device_id: gd.device_id,
            delivered_at: receipt_map.get(&gd.device_id).copied(),
        })
        .collect();

    let delivered_count = per_device
        .iter()
        .filter(|d| d.delivered_at.is_some())
        .count() as i64;

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &DeliveryStatus {
            message_id,
            total_devices,
            delivered_count,
            per_device,
        },
    ))
}

/// Невостребованные welcome'ы для устройств текущего пользователя.
///
/// Фильтрует по `recipient_device_id = ctx.device.id` (каждое устройство
/// pull'ит свои welcome'ы).
///
/// # Errors
///
/// - `500` — ошибка БД.
#[allow(clippy::cast_possible_wrap)]
pub async fn list_welcomes(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Query(query): Query<ListWelcomesQuery>,
) -> Result<Response, AppError> {
    let mut cond = Condition::all()
        .add(mls_welcomes::Column::RecipientDeviceId.eq(ctx.device.id))
        .add(mls_welcomes::Column::ConsumedAt.is_null());

    if let Some(since_id) = query.since_id {
        cond = cond.add(mls_welcomes::Column::Id.gt(since_id));
    }

    let welcomes = mls_welcomes::Entity::find()
        .filter(cond)
        .order_by_asc(mls_welcomes::Column::Id)
        .all(&state.db)
        .await?
        .into_iter()
        .map(|w| PendingWelcome {
            id: w.id,
            group_id: w.group_id,
            recipient_device_id: w.recipient_device_id,
            epoch: w.epoch,
            welcome_ciphertext: w.welcome_ciphertext,
            created_at: w.created_at,
        })
        .collect();

    Ok(typed_response(
        &headers,
        StatusCode::OK,
        &ListWelcomesResponse { welcomes },
    ))
}

/// Подтверждение получения welcome (пометить consumed).
///
/// # Errors
///
/// - `404` — welcome не найден.
/// - `403` — welcome не предназначен этому устройству.
/// - `500` — ошибка БД.
pub async fn ack_welcome(
    State(state): State<AppState>,
    _headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(welcome_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let welcome = mls_welcomes::Entity::find_by_id(welcome_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    if welcome.recipient_device_id != ctx.device.id {
        return Err(AppError::Forbidden);
    }

    let mut active: mls_welcomes::ActiveModel = welcome.into();
    active.consumed_at = Set(Some(now_secs()));
    active.update(&state.db).await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Добавить реакцию (дедуп через ON CONFLICT DO NOTHING).
///
/// # Errors
///
/// - `404` — сообщение не найдено.
/// - `403` — не участник группы.
/// - `500` — ошибка БД.
pub async fn add_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path(message_id): Path<Uuid>,
    body: Bytes,
) -> Result<Response, AppError> {
    let req: AddReactionRequest = decode_body(&headers, &body)?;
    let now = now_secs();

    // Найти сообщение
    let message = mls_messages::Entity::find_by_id(message_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;

    // Проверить membership
    let is_member = mls_group_members::Entity::find()
        .filter(
            Condition::all()
                .add(mls_group_members::Column::GroupId.eq(message.group_id))
                .add(mls_group_members::Column::UserId.eq(ctx.user.id))
                .add(mls_group_members::Column::LeftAtEpoch.is_null()),
        )
        .one(&state.db)
        .await?
        .is_some();

    if !is_member {
        return Err(AppError::GroupMembershipRequired);
    }

    // INSERT with ON CONFLICT DO NOTHING
    let _ = reactions::Entity::insert(reactions::ActiveModel {
        message_id: Set(message_id),
        user_id: Set(ctx.user.id),
        reaction_blind_index: Set(req.reaction_blind_index),
        sender_device_id: Set(ctx.device.id),
        applied_at_epoch: Set(req.applied_at_epoch),
        created_at: Set(now),
    })
    .on_conflict(
        OnConflict::columns([
            reactions::Column::MessageId,
            reactions::Column::UserId,
            reactions::Column::ReactionBlindIndex,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec(&state.db)
    .await;

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Удалить реакцию.
///
/// # Errors
///
/// - `500` — ошибка БД.
pub async fn remove_reaction(
    State(state): State<AppState>,
    _headers: HeaderMap,
    CurrentAuth(ctx): CurrentAuth,
    Path((message_id, blind_index_hex)): Path<(Uuid, String)>,
) -> Result<Response, AppError> {
    let blind_index = hex::decode(&blind_index_hex).map_err(|_| {
        AppError::BadRequest("invalid hex blind_index".into())
    })?;

    reactions::Entity::delete_many()
        .filter(
            Condition::all()
                .add(reactions::Column::MessageId.eq(message_id))
                .add(reactions::Column::UserId.eq(ctx.user.id))
                .add(reactions::Column::ReactionBlindIndex.eq(blind_index)),
        )
        .exec(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
