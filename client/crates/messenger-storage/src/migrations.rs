//! Local database schema migrations.

/// Schema version 1 DDL.
pub fn schema_v1() -> Vec<&'static str> {
    vec![
        "CREATE TABLE IF NOT EXISTS identity (
            user_id BLOB PRIMARY KEY NOT NULL,
            identity_secret_key_wrapped BLOB NOT NULL,
            identity_public_key BLOB NOT NULL,
            device_signing_secret_key_wrapped BLOB NOT NULL,
            device_signing_public_key BLOB NOT NULL,
            device_hpke_secret_key_wrapped BLOB NOT NULL,
            device_hpke_public_key BLOB NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS mls_groups (
            device_id BLOB NOT NULL,
            group_id BLOB NOT NULL,
            state_blob BLOB NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (device_id, group_id)
        )",
        "CREATE TABLE IF NOT EXISTS chats (
            group_id BLOB PRIMARY KEY NOT NULL,
            chat_type TEXT NOT NULL,
            display_name TEXT,
            avatar_blob BLOB,
            last_message_at INTEGER,
            unread_count INTEGER NOT NULL DEFAULT 0,
            archived INTEGER NOT NULL DEFAULT 0,
            pinned INTEGER NOT NULL DEFAULT 0,
            mute_until INTEGER,
            updated_at INTEGER NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS messages (
            id BLOB PRIMARY KEY NOT NULL,
            group_id BLOB NOT NULL,
            sender_user_id BLOB NOT NULL,
            sender_device_id BLOB NOT NULL,
            wire_format TEXT NOT NULL,
            ciphertext BLOB NOT NULL,
            plaintext BLOB,
            content_type TEXT,
            reply_to_message_id BLOB,
            thread_root_id BLOB,
            edited_at INTEGER,
            deleted_at INTEGER,
            created_at INTEGER NOT NULL
        )",
        "CREATE INDEX IF NOT EXISTS idx_messages_group_id ON messages(group_id, id)",
        "CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_root_id, id) WHERE thread_root_id IS NOT NULL",
        "CREATE TABLE IF NOT EXISTS keypackages_local (
            id BLOB PRIMARY KEY NOT NULL,
            init_key_hash BLOB NOT NULL UNIQUE,
            secret_keys_wrapped BLOB NOT NULL,
            expires_at INTEGER NOT NULL,
            is_last_resort INTEGER NOT NULL DEFAULT 0,
            published INTEGER NOT NULL DEFAULT 0,
            consumed INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS attachments_meta (
            attachment_id BLOB PRIMARY KEY NOT NULL,
            message_id BLOB,
            decryption_key_wrapped BLOB NOT NULL,
            mime TEXT,
            display_filename TEXT,
            padded_size INTEGER,
            real_size INTEGER,
            created_at INTEGER NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS reactions_local (
            message_id BLOB NOT NULL,
            user_id BLOB NOT NULL,
            emoji TEXT NOT NULL,
            applied_at INTEGER NOT NULL,
            PRIMARY KEY (message_id, user_id, emoji)
        )",
    ]
}
