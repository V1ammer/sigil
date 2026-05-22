//! English translation dictionary.

use std::collections::HashMap;

/// Build the English translation map.
#[must_use]
pub fn en_dict() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("app.name", "Secure Messenger");
    m.insert("app.description", "Corporate E2E Messenger");

    // Connection screen
    m.insert("connect.title", "Connect to Server");
    m.insert("connect.placeholder", "https://messenger.company.com");
    m.insert("connect.button", "Connect");
    m.insert("connect.whatsThis", "What is this?");
    m.insert("connect.help.title", "What is a server address?");
    m.insert("connect.help.description", "This is the address of your corporate messenger server provided by your administrator. All messages are encrypted and stored only on this server.");
    m.insert("connect.error.unavailable", "Server unavailable");
    m.insert("connect.error.invalid", "Invalid server address");

    // Login screen
    m.insert("login.title", "Sign in to");
    m.insert("login.token.title", "Sign in with token");
    m.insert("login.token.description", "You received an invitation from admin");
    m.insert("login.qr.title", "Sign in from another device");
    m.insert("login.qr.description", "Show QR code from your old device");
    m.insert("login.newDevice", "Is this a new device for an existing account? Use QR.");

    // Token login
    m.insert("token.title", "Enter invitation token");
    m.insert("token.placeholder", "XXXX-XXXX-XXXX-XXXX");
    m.insert("token.hint", "Token provided by server administrator");
    m.insert("token.continue", "Continue");
    m.insert("token.error.invalid", "Invalid invitation token");
    m.insert("token.error.expired", "Token has expired");
    m.insert("token.error.exhausted", "Token has been used maximum number of times");

    // QR login
    m.insert("qr.title", "Scan QR code");
    m.insert("qr.instruction", "Open the messenger on the device where you are already signed in, go to Settings → Devices → Add device, and scan this code.");
    m.insert("qr.validFor", "Valid for");
    m.insert("qr.refresh", "Refresh QR");
    m.insert("qr.requestId", "Request ID");
    m.insert("qr.waiting", "Waiting for confirmation...");
    m.insert("qr.success", "Device added");

    // Registration
    m.insert("register.title", "Create account");
    m.insert("register.username", "Username");
    m.insert("register.username.hint", "Only letters, numbers and underscore");
    m.insert("register.displayName", "Display name");
    m.insert("register.avatar", "Avatar");
    m.insert("register.avatar.hint", "Drag an image or click to select");
    m.insert("register.create", "Create account");
    m.insert("register.privacy", "This data is stored encrypted. Server only sees username hash.");
    m.insert("register.username.taken", "This username is already taken");
    m.insert("register.username.available", "Username is available");

    // Chats
    m.insert("chats.title", "Chats");
    m.insert("chats.search", "Search...");
    m.insert("chats.search.hint", "Enter @username to search for a user");
    m.insert("chats.empty", "No chats");
    m.insert("chats.empty.hint", "Start your first chat");
    m.insert("chats.new", "New chat");
    m.insert("chats.new.direct", "Direct");
    m.insert("chats.new.group", "Group");
    m.insert("chats.archive", "Archive");
    m.insert("chats.filter.all", "All");
    m.insert("chats.filter.unread", "Unread");
    m.insert("chats.filter.direct", "Direct");
    m.insert("chats.filter.groups", "Groups");

    // Chat actions
    m.insert("chat.pin", "Pin");
    m.insert("chat.unpin", "Unpin");
    m.insert("chat.mute", "Mute");
    m.insert("chat.unmute", "Unmute");
    m.insert("chat.mute.1h", "1 hour");
    m.insert("chat.mute.8h", "8 hours");
    m.insert("chat.mute.1d", "1 day");
    m.insert("chat.mute.forever", "Forever");
    m.insert("chat.markRead", "Mark as read");
    m.insert("chat.archiveChat", "Archive");
    m.insert("chat.clearHistory", "Clear history");
    m.insert("chat.delete", "Delete chat");
    m.insert("chat.leave", "Leave group");
    m.insert("chat.profile", "Profile");
    m.insert("chat.search", "Search in chat");
    m.insert("chat.pinChat", "Pin chat");
    m.insert("chat.unpinChat", "Unpin chat");
    m.insert("chat.direct", "Direct");
    m.insert("chat.members", "members");

    // Messages
    m.insert("message.placeholder", "Message");
    m.insert("message.reply", "Reply");
    m.insert("message.replyThread", "Reply in thread");
    m.insert("message.react", "React");
    m.insert("message.edit", "Edit");
    m.insert("message.copy", "Copy");
    m.insert("message.forward", "Forward");
    m.insert("message.save", "Save");
    m.insert("message.pin", "Pin");
    m.insert("message.delete", "Delete");
    m.insert("message.deleteForAll", "Delete for everyone");
    m.insert("message.deleteForMe", "Delete for me");
    m.insert("message.deleted", "Message deleted");
    m.insert("message.edited", "edited");
    m.insert("message.replies", "replies");
    m.insert("message.transcribe", "Transcribe");
    m.insert("message.transcribe.unavailable", "Transcription unavailable in this prototype");

    // Attachments
    m.insert("attach.photo", "Photo");
    m.insert("attach.file", "File");
    m.insert("attach.voice", "Voice");

    // Welcome state
    m.insert("welcome.title", "Select a chat");
    m.insert("welcome.hint", "Select a chat from the left or start a new one");

    // Security
    m.insert("security.title", "Security");
    m.insert("security.changes", "Security changes");
    m.insert("security.safetyNumber", "Safety Number");
    m.insert("security.compare", "Compare in person");
    m.insert("security.devices", "devices");
    m.insert("security.newDevice", "New device");
    m.insert("security.warning", "Security changed — contact has a new device. Verify safety number.");

    // Settings
    m.insert("settings.title", "Settings");
    m.insert("settings.account", "Account");
    m.insert("settings.devices", "Devices");
    m.insert("settings.appearance", "Appearance");
    m.insert("settings.notifications", "Notifications");
    m.insert("settings.privacy", "Privacy");
    m.insert("settings.admin", "Server management");
    m.insert("settings.about", "About");
    m.insert("settings.logout", "Sign out");

    // Account settings
    m.insert("account.displayName", "Display name");
    m.insert("account.username", "Username");
    m.insert("account.username.hint", "Username cannot be changed easily; contact admin");
    m.insert("account.bio", "Bio");
    m.insert("account.avatar.change", "Change avatar");
    m.insert("account.visibility", "Profile is only visible to people you share chats with");
    m.insert("account.save", "Save");
    m.insert("settings.account.description", "Manage your profile and personal data");
    m.insert("settings.account.bioPlaceholder", "Tell us about yourself...");
    m.insert("settings.account.privacyNote", "Profile is only visible to people you share chats with");

    // Devices
    m.insert("devices.title", "My devices");
    m.insert("devices.current", "This device");
    m.insert("devices.add", "Add device");
    m.insert("devices.logout", "Sign out on this device");
    m.insert("devices.lastActive", "Last active");
    m.insert("devices.added", "Added");
    m.insert("settings.devices.description", "Manage connected devices");
    m.insert("settings.devices.current", "Current device");
    m.insert("settings.devices.other", "Other devices");
    m.insert("settings.devices.thisDevice", "This device");
    m.insert("settings.devices.logoutConfirm", "Sign out on this device?");
    m.insert("settings.devices.logoutDescription", "You will be signed out from this device. A QR code or invitation will be required to sign in again.");
    m.insert("settings.devices.scanQR", "Scan QR on new device");
    m.insert("settings.devices.scanQRDescription", "Point camera at QR code displayed on new device");
    m.insert("settings.devices.cameraPlaceholder", "Camera unavailable in prototype");
    m.insert("settings.devices.enterManually", "Enter manually");
    m.insert("settings.devices.provisioningId", "Request ID");
    m.insert("settings.devices.confirmAdd", "Confirm device addition");
    m.insert("settings.devices.confirmAddDescription", "You are about to add a new device with ID:");
    m.insert("settings.devices.addDevice", "Add device");
    m.insert("settings.devices.added", "Added");
    m.insert("settings.devices.currentDevice", "Current device");
    m.insert("settings.devices.lastActive", "Last active");
    m.insert("settings.devices.newDevice", "New device");
    m.insert("settings.devices.revoke", "Revoke");
    m.insert("settings.devices.title", "Devices");

    // Appearance
    m.insert("appearance.theme", "Theme");
    m.insert("appearance.theme.system", "System");
    m.insert("appearance.theme.light", "Light");
    m.insert("appearance.theme.dark", "Dark");
    m.insert("appearance.language", "Language");
    m.insert("appearance.fontSize", "Font size");
    m.insert("appearance.fontSize.small", "Small");
    m.insert("appearance.fontSize.medium", "Medium");
    m.insert("appearance.fontSize.large", "Large");
    m.insert("settings.appearance.description", "Customize app appearance");
    m.insert("settings.appearance.fontLarge", "Large");
    m.insert("settings.appearance.fontMedium", "Medium");
    m.insert("settings.appearance.fontSize", "Font size");
    m.insert("settings.appearance.fontSmall", "Small");
    m.insert("settings.appearance.langEn", "English");
    m.insert("settings.appearance.langRu", "Russian");
    m.insert("settings.appearance.language", "Language");
    m.insert("settings.appearance.theme", "Theme");
    m.insert("settings.appearance.title", "Appearance");

    // Notifications
    m.insert("notifications.enable", "Enable notifications");
    m.insert("notifications.sound", "Sound");
    m.insert("notifications.preview", "Message preview");
    m.insert("notifications.readReceipts", "Send read receipts");
    m.insert("notifications.readReceipts.hint", "If disabled, senders won't know you read their messages");
    m.insert("settings.notifications.description", "Configure message notifications");
    m.insert("settings.notifications.enableDescription", "Receive notifications for new messages");
    m.insert("settings.notifications.soundDescription", "Play sound when receiving messages");
    m.insert("settings.notifications.previewDescription", "Show message content in notifications");
    m.insert("settings.notifications.readReceiptsDescription", "If disabled, senders will not know you read their messages");
    m.insert("settings.notifications.enable", "Enable notifications");
    m.insert("settings.notifications.enableDesc", "Receive notifications");
    m.insert("settings.notifications.preview", "Message preview");
    m.insert("settings.notifications.previewDesc", "Show content");
    m.insert("settings.notifications.readReceipts", "Read receipts");
    m.insert("settings.notifications.readReceiptsDesc", "Send read receipts");
    m.insert("settings.notifications.sound", "Sound");
    m.insert("settings.notifications.soundDesc", "Sound notification");
    m.insert("settings.notifications.title", "Notifications");

    // Privacy
    m.insert("privacy.historyRetention", "Local history retention");
    m.insert("privacy.autoDelete", "Auto-delete messages in new chats");
    m.insert("privacy.1week", "1 week");
    m.insert("privacy.1month", "1 month");
    m.insert("privacy.3months", "3 months");
    m.insert("privacy.1year", "1 year");
    m.insert("privacy.forever", "Forever");
    m.insert("privacy.off", "Off");
    m.insert("privacy.1day", "1 day");
    m.insert("settings.privacy.description", "Privacy and data storage settings");
    m.insert("settings.privacy.historyRetentionDescription", "How long to keep messages on this device");
    m.insert("settings.privacy.autoDeleteDescription", "Auto-delete messages in new chats");
    m.insert("settings.privacy.autoDelete", "Auto-delete");
    m.insert("settings.privacy.autoDelete24h", "24 hours");
    m.insert("settings.privacy.autoDelete30d", "30 days");
    m.insert("settings.privacy.autoDelete7d", "7 days");
    m.insert("settings.privacy.autoDelete90d", "90 days");
    m.insert("settings.privacy.autoDeleteHint", "Auto-delete messages");
    m.insert("settings.privacy.autoDeleteOff", "Off");
    m.insert("settings.privacy.historyForever", "Forever");
    m.insert("settings.privacy.historyHint", "How long to keep messages");
    m.insert("settings.privacy.historyMonth", "1 month");
    m.insert("settings.privacy.historyRetention", "History retention");
    m.insert("settings.privacy.historyWeek", "1 week");
    m.insert("settings.privacy.historyYear", "1 year");
    m.insert("settings.privacy.title", "Privacy");

    // Admin invites
    m.insert("admin.invites", "Invitations");
    m.insert("admin.invites.create", "Create invitation");
    m.insert("admin.invites.role", "Role");
    m.insert("admin.invites.maxUses", "Max uses");
    m.insert("admin.invites.expiry", "Expiry");
    m.insert("admin.invites.expiry.1h", "1 hour");
    m.insert("admin.invites.expiry.1d", "1 day");
    m.insert("admin.invites.expiry.1w", "1 week");
    m.insert("admin.invites.expiry.1m", "1 month");
    m.insert("admin.invites.expiry.never", "No expiry");
    m.insert("admin.invites.created", "Invitation created");
    m.insert("admin.invites.copy", "Copy");
    m.insert("admin.invites.share", "Share link");
    m.insert("admin.invites.showQr", "Show QR");
    m.insert("admin.invites.onceWarning", "This token is shown only once. Save it now.");
    m.insert("admin.invites.revoke", "Revoke");
    m.insert("admin.invites.uses", "uses");
    m.insert("admin.invites.expiresIn", "expires in");
    m.insert("admin.invites.status", "Status");
    m.insert("settings.adminInvites.actions", "Actions");
    m.insert("settings.adminInvites.cancel", "Cancel");
    m.insert("settings.adminInvites.create", "Create invitation");
    m.insert("settings.adminInvites.createDesc", "Create a new user invitation");
    m.insert("settings.adminInvites.createTitle", "New invitation");
    m.insert("settings.adminInvites.description", "Manage invitations");
    m.insert("settings.adminInvites.expires", "Expires");
    m.insert("settings.adminInvites.maxUses", "Max uses");
    m.insert("settings.adminInvites.revoke", "Revoke");
    m.insert("settings.adminInvites.role", "Role");
    m.insert("settings.adminInvites.roleAdmin", "Admin");
    m.insert("settings.adminInvites.roleUser", "User");
    m.insert("settings.adminInvites.status", "Status");
    m.insert("settings.adminInvites.title", "Invitations");
    m.insert("settings.adminInvites.token", "Token");
    m.insert("settings.adminInvites.uses", "Uses");

    // Admin users
    m.insert("admin.users", "Users");
    m.insert("admin.users.suspend", "Suspend");
    m.insert("admin.users.unsuspend", "Unsuspend");
    m.insert("settings.adminUsers.actions", "Actions");
    m.insert("settings.adminUsers.created", "Created");
    m.insert("settings.adminUsers.description", "Manage users");
    m.insert("settings.adminUsers.lastActive", "Last active");
    m.insert("settings.adminUsers.role", "Role");
    m.insert("settings.adminUsers.status", "Status");
    m.insert("settings.adminUsers.suspend", "Suspend");
    m.insert("settings.adminUsers.title", "Users");
    m.insert("settings.adminUsers.unsuspend", "Unsuspend");
    m.insert("settings.adminUsers.user", "User");

    // Admin stats
    m.insert("admin.stats", "Server statistics");
    m.insert("admin.stats.users", "Users");
    m.insert("admin.stats.activeDay", "Active in 24h");
    m.insert("admin.stats.groups", "Groups");
    m.insert("admin.stats.messagesDay", "Messages today");

    // About
    m.insert("about.version", "Version");
    m.insert("about.server", "Server address");
    m.insert("about.license", "License");
    m.insert("settings.about.copyright", "© 2024 Secure Messenger. All rights reserved.");
    m.insert("settings.about.description", "App and server information");
    m.insert("settings.about.license", "AGPL-3.0");
    m.insert("settings.about.serverAddress", "Server address");
    m.insert("settings.about.serverName", "Server name");
    m.insert("settings.about.title", "About");
    m.insert("settings.about.version", "Version");

    // Profile
    m.insert("profile.title", "Profile");
    m.insert("profile.participants", "participants");
    m.insert("profile.block", "Block");
    m.insert("profile.addMembers", "Add members");

    // Errors and states
    m.insert("error.title", "Error");
    m.insert("error.retry", "Retry");
    m.insert("error.offline", "No connection to server. Messages will be sent when connection is restored.");
    m.insert("loading", "Loading...");
    m.insert("cancel", "Cancel");
    m.insert("confirm", "Confirm");
    m.insert("back", "Back");
    m.insert("close", "Close");

    // Time
    m.insert("time.today", "Today");
    m.insert("time.yesterday", "Yesterday");
    m.insert("time.daysAgo", "days ago");
    m.insert("time.seconds", "s");
    m.insert("time.minutes", "min");
    m.insert("time.hours", "h");
    m.insert("time.days", "d");

    // Scan
    m.insert("scan.title", "Scan QR on new device");
    m.insert("scan.manual", "Enter manually");
    m.insert("scan.confirm.title", "Confirm device addition");
    m.insert("scan.confirm", "Confirm");

    // Settings sub-sections
    m.insert("settings.account.save", "Save");
    m.insert("settings.account.safetyHint", "Compare safety number in person");
    m.insert("settings.account.safetyNumber", "Safety Number");
    m.insert("settings.account.title", "Account");
    m.insert("settings.account.usernameHint", "Username cannot be changed");

    // Settings — voice
    m.insert("settings.voice.description", "Voice message settings");
    m.insert("settings.voice.title", "Voice");

    // Sidebar
    m.insert("sidebar.chatList.archive", "Archive");
    m.insert("sidebar.chatList.clearHistory", "Clear history");
    m.insert("sidebar.chatList.deleteChat", "Delete chat");
    m.insert("sidebar.chatList.leaveGroup", "Leave group");
    m.insert("sidebar.chatList.logout", "Sign out");
    m.insert("sidebar.chatList.markRead", "Mark read");
    m.insert("sidebar.chatList.mute", "Mute");
    m.insert("sidebar.chatList.newChat", "New chat");
    m.insert("sidebar.chatList.noChats", "No chats");
    m.insert("sidebar.chatList.pin", "Pin");
    m.insert("sidebar.chatList.search", "Search...");
    m.insert("sidebar.chatList.settings", "Settings");
    m.insert("sidebar.chatList.unmute", "Unmute");
    m.insert("sidebar.chatList.unpin", "Unpin");

    // Common
    m.insert("common.cancel", "Cancel");
    m.insert("common.save", "Save");
    m.insert("common.delete", "Delete");
    m.insert("common.close", "Close");
    m.insert("common.confirm", "Confirm");

    // Error messages
    m.insert("error.network", "Network error");
    m.insert("error.invite_invalid", "Invalid invite token");
    m.insert("error.invite_expired", "Invite token has expired");
    m.insert("error.invite_exhausted", "Invite token has been exhausted");
    m.insert("error.username_taken", "Username is already taken");

    m
}
