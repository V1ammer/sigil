//! Chat list state — the sidebar's primary data.

use leptos::prelude::*;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum AvatarSource {
    Initials(String),
    Image(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatType {
    Direct,
    Group,
}

#[derive(Clone, Debug)]
pub struct Chat {
    pub group_id: Uuid,
    pub chat_type: ChatType,
    pub display_name: String,
    pub avatar: Option<AvatarSource>,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<i64>,
    pub unread_count: u32,
    pub muted: bool,
    pub pinned: bool,
    pub current_epoch: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatFilter {
    All,
    Unread,
    Direct,
    Groups,
    Archived,
}

/// Reactive chat-list state.
#[derive(Clone)]
pub struct ChatsState {
    pub chats: RwSignal<Vec<Chat>>,
    pub selected: RwSignal<Option<Uuid>>,
    pub search: RwSignal<String>,
    pub filter: RwSignal<ChatFilter>,
}

impl ChatsState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            chats: RwSignal::new(Vec::new()),
            selected: RwSignal::new(None),
            search: RwSignal::new(String::new()),
            filter: RwSignal::new(ChatFilter::All),
        }
    }

    /// Returns a memoised derived signal that filters, searches and sorts the
    /// chat list.
    pub fn filtered(&self) -> impl Fn() -> Vec<Chat> + 'static {
        let chats = self.chats;
        let search = self.search;
        let filter = self.filter;
        move || {
            let mut list = chats.get();
            let s = search.get().to_lowercase();
            if !s.is_empty() {
                list.retain(|c| c.display_name.to_lowercase().contains(&s));
            }
            match filter.get() {
                ChatFilter::Unread => list.retain(|c| c.unread_count > 0),
                ChatFilter::Direct => list.retain(|c| c.chat_type == ChatType::Direct),
                ChatFilter::Groups => list.retain(|c| c.chat_type == ChatType::Group),
                ChatFilter::Archived => {
                    // TODO archived flag when we have it
                    list.clear();
                }
                ChatFilter::All => {}
            }
            // pinned → top, then by last_message_at desc
            list.sort_by(|a, b| {
                b.pinned
                    .cmp(&a.pinned)
                    .then(b.last_message_at.cmp(&a.last_message_at))
            });
            list
        }
    }
}

impl Default for ChatsState {
    fn default() -> Self {
        Self::new()
    }
}
