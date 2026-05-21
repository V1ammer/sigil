//! Simple i18n with ru/en dictionary.

use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Language {
    Ru,
    En,
}

impl Default for Language {
    fn default() -> Self {
        Self::Ru
    }
}

impl Language {
    pub fn from_str(s: &str) -> Self {
        match s {
            "en" => Self::En,
            _ => Self::Ru,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ru => "ru",
            Self::En => "en",
        }
    }
}

static TRANSLATIONS: LazyLock<HashMap<&'static str, [&'static str; 2]>> =
    LazyLock::new(|| {
        let mut m: HashMap<&str, [&str; 2]> = HashMap::new();
        m.insert("app.name", ["Secure Messenger", "Secure Messenger"]);
        m.insert("app.description", ["Корпоративный E2E мессенджер", "Corporate E2E Messenger"]);

        // Connection screen
        m.insert("connect.title", ["Подключение к серверу", "Connect to Server"]);
        m.insert("connect.placeholder", ["https://messenger.company.com", "https://messenger.company.com"]);
        m.insert("connect.button", ["Подключиться", "Connect"]);
        m.insert("connect.whatsThis", ["Что это?", "What is this?"]);
        m.insert("connect.help.title", ["Что такое адрес сервера?", "What is a server address?"]);
        m.insert("connect.help.description", ["Это адрес корпоративного сервера мессенджера, предоставленный вашим администратором. Все сообщения зашифрованы и хранятся только на этом сервере.", "This is the address of your corporate messenger server provided by your administrator. All messages are encrypted and stored only on this server."]);
        m.insert("connect.error.unavailable", ["Сервер недоступен", "Server unavailable"]);
        m.insert("connect.error.invalid", ["Неверный адрес сервера", "Invalid server address"]);

        // Login screen
        m.insert("login.title", ["Войти на", "Sign in to"]);
        m.insert("login.token.title", ["Войти по токену", "Sign in with token"]);
        m.insert("login.token.description", ["Получили приглашение от админа", "You received an invitation from admin"]);
        m.insert("login.qr.title", ["Войти с другого устройства", "Sign in from another device"]);
        m.insert("login.qr.description", ["Покажите QR со старого устройства", "Show QR code from your old device"]);
        m.insert("login.newDevice", ["Это новое устройство существующего аккаунта? Используйте QR.", "Is this a new device for an existing account? Use QR."]);

        // Token login
        m.insert("token.title", ["Введите токен приглашения", "Enter invitation token"]);
        m.insert("token.placeholder", ["XXXX-XXXX-XXXX-XXXX", "XXXX-XXXX-XXXX-XXXX"]);
        m.insert("token.hint", ["Токен предоставил администратор сервера", "Token provided by server administrator"]);
        m.insert("token.continue", ["Продолжить", "Continue"]);
        m.insert("token.error.invalid", ["Неверный токен приглашения", "Invalid invitation token"]);
        m.insert("token.error.expired", ["Срок действия токена истёк", "Token has expired"]);
        m.insert("token.error.exhausted", ["Токен уже использован максимальное число раз", "Token has been used maximum number of times"]);

        // QR login
        m.insert("qr.title", ["Отсканируйте QR код", "Scan QR code"]);
        m.insert("qr.instruction", ["Откройте мессенджер на устройстве где вы уже вошли, нажмите Настройки → Устройства → Добавить устройство, и отсканируйте этот код.", "Open the messenger on the device where you are already signed in, go to Settings → Devices → Add device, and scan this code."]);
        m.insert("qr.validFor", ["Действителен ещё", "Valid for"]);
        m.insert("qr.refresh", ["Обновить QR", "Refresh QR"]);
        m.insert("qr.requestId", ["ID запроса", "Request ID"]);
        m.insert("qr.waiting", ["Ждём подтверждения...", "Waiting for confirmation..."]);
        m.insert("qr.success", ["Устройство добавлено", "Device added"]);

        // Registration
        m.insert("register.title", ["Создание аккаунта", "Create account"]);
        m.insert("register.username", ["Имя пользователя", "Username"]);
        m.insert("register.username.hint", ["Только латинские буквы, цифры и подчёркивание", "Only letters, numbers and underscore"]);
        m.insert("register.displayName", ["Отображаемое имя", "Display name"]);
        m.insert("register.avatar", ["Аватар", "Avatar"]);
        m.insert("register.avatar.hint", ["Перетащите изображение или нажмите для выбора", "Drag an image or click to select"]);
        m.insert("register.create", ["Создать аккаунт", "Create account"]);
        m.insert("register.privacy", ["Эти данные хранятся зашифрованно. Сервер видит только хэш username.", "This data is stored encrypted. Server only sees username hash."]);
        m.insert("register.username.taken", ["Это имя пользователя уже занято", "This username is already taken"]);
        m.insert("register.username.available", ["Имя пользователя доступно", "Username is available"]);

        // Chats
        m.insert("chats.title", ["Чаты", "Chats"]);
        m.insert("chats.search", ["Поиск...", "Search..."]);
        m.insert("chats.search.hint", ["Введите @username для поиска пользователя", "Enter @username to search for a user"]);
        m.insert("chats.empty", ["Нет чатов", "No chats"]);
        m.insert("chats.empty.hint", ["Начните первый чат", "Start your first chat"]);
        m.insert("chats.new", ["Новый чат", "New chat"]);
        m.insert("chats.new.direct", ["Личный", "Direct"]);
        m.insert("chats.new.group", ["Групповой", "Group"]);
        m.insert("chats.archive", ["Архив", "Archive"]);

        // Chat actions
        m.insert("chat.pin", ["Закрепить", "Pin"]);
        m.insert("chat.unpin", ["Открепить", "Unpin"]);
        m.insert("chat.mute", ["Без звука", "Mute"]);
        m.insert("chat.unmute", ["Включить звук", "Unmute"]);
        m.insert("chat.mute.1h", ["1 час", "1 hour"]);
        m.insert("chat.mute.8h", ["8 часов", "8 hours"]);
        m.insert("chat.mute.1d", ["1 день", "1 day"]);
        m.insert("chat.mute.forever", ["Навсегда", "Forever"]);
        m.insert("chat.markRead", ["Прочитать", "Mark as read"]);
        m.insert("chat.archiveChat", ["Архивировать", "Archive"]);
        m.insert("chat.clearHistory", ["Очистить историю", "Clear history"]);
        m.insert("chat.delete", ["Удалить чат", "Delete chat"]);
        m.insert("chat.leave", ["Покинуть группу", "Leave group"]);
        m.insert("chat.profile", ["Профиль", "Profile"]);
        m.insert("chat.search", ["Поиск по чату", "Search in chat"]);

        // Messages
        m.insert("message.placeholder", ["Сообщение", "Message"]);
        m.insert("message.reply", ["Ответить", "Reply"]);
        m.insert("message.replyThread", ["Ответить в треде", "Reply in thread"]);
        m.insert("message.react", ["Реакция", "React"]);
        m.insert("message.edit", ["Изменить", "Edit"]);
        m.insert("message.copy", ["Копировать", "Copy"]);
        m.insert("message.forward", ["Переслать", "Forward"]);
        m.insert("message.save", ["Сохранить", "Save"]);
        m.insert("message.pin", ["Закрепить", "Pin"]);
        m.insert("message.delete", ["Удалить", "Delete"]);
        m.insert("message.deleteForAll", ["Удалить у всех", "Delete for everyone"]);
        m.insert("message.deleteForMe", ["Удалить у меня", "Delete for me"]);
        m.insert("message.deleted", ["Сообщение удалено", "Message deleted"]);
        m.insert("message.edited", ["изменено", "edited"]);
        m.insert("message.replies", ["ответов", "replies"]);
        m.insert("message.transcribe", ["Расшифровать", "Transcribe"]);
        m.insert("message.transcribe.unavailable", ["Транскрибация недоступна в этом прототипе", "Transcription unavailable in this prototype"]);

        // Attachments
        m.insert("attach.photo", ["Фото", "Photo"]);
        m.insert("attach.file", ["Файл", "File"]);
        m.insert("attach.voice", ["Голосовое", "Voice"]);

        // Welcome state
        m.insert("welcome.title", ["Выберите чат", "Select a chat"]);
        m.insert("welcome.hint", ["Выберите чат слева или начните новый", "Select a chat from the left or start a new one"]);

        // Security
        m.insert("security.title", ["Безопасность", "Security"]);
        m.insert("security.changes", ["Изменения безопасности", "Security changes"]);
        m.insert("security.safetyNumber", ["Safety Number", "Safety Number"]);
        m.insert("security.compare", ["Сравнить лично", "Compare in person"]);
        m.insert("security.devices", ["устройств", "devices"]);
        m.insert("security.newDevice", ["Новое устройство", "New device"]);
        m.insert("security.warning", ["Безопасность изменилась — у контакта появилось новое устройство. Сверьте safety number.", "Security changed — contact has a new device. Verify safety number."]);

        // Settings
        m.insert("settings.title", ["Настройки", "Settings"]);
        m.insert("settings.account", ["Аккаунт", "Account"]);
        m.insert("settings.devices", ["Устройства", "Devices"]);
        m.insert("settings.appearance", ["Внешний вид", "Appearance"]);
        m.insert("settings.notifications", ["Уведомления", "Notifications"]);
        m.insert("settings.privacy", ["Приватность", "Privacy"]);
        m.insert("settings.admin", ["Управление сервером", "Server management"]);
        m.insert("settings.about", ["О приложении", "About"]);
        m.insert("settings.logout", ["Выйти", "Sign out"]);

        // Account
        m.insert("account.displayName", ["Отображаемое имя", "Display name"]);
        m.insert("account.username", ["Имя пользователя", "Username"]);
        m.insert("account.username.hint", ["Username нельзя изменить просто так; обратитесь к админу", "Username cannot be changed easily; contact admin"]);
        m.insert("account.bio", ["О себе", "Bio"]);
        m.insert("account.avatar.change", ["Изменить аватар", "Change avatar"]);
        m.insert("account.visibility", ["Профиль виден только людям с которыми у вас есть общие чаты", "Profile is only visible to people you share chats with"]);
        m.insert("account.save", ["Сохранить", "Save"]);
        m.insert("settings.account.description", ["Управление профилем и личными данными", "Manage your profile and personal data"]);
        m.insert("settings.account.bioPlaceholder", ["Расскажите о себе...", "Tell us about yourself..."]);
        m.insert("settings.account.privacyNote", ["Профиль виден только людям с которыми у вас есть общие чаты", "Profile is only visible to people you share chats with"]);

        // Devices
        m.insert("devices.title", ["Мои устройства", "My devices"]);
        m.insert("devices.current", ["Это устройство", "This device"]);
        m.insert("devices.add", ["Добавить устройство", "Add device"]);
        m.insert("devices.logout", ["Выйти на этом устройстве", "Sign out on this device"]);
        m.insert("devices.lastActive", ["Последняя активность", "Last active"]);
        m.insert("devices.added", ["Добавлено", "Added"]);
        m.insert("settings.devices.description", ["Управление подключёнными устройствами", "Manage connected devices"]);
        m.insert("settings.devices.current", ["Текущее устройство", "Current device"]);
        m.insert("settings.devices.other", ["Другие устройства", "Other devices"]);
        m.insert("settings.devices.thisDevice", ["Это устройство", "This device"]);
        m.insert("settings.devices.logoutConfirm", ["Выйти на этом устройстве?", "Sign out on this device?"]);
        m.insert("settings.devices.logoutDescription", ["Вы будете отключены от этого устройства. Для повторного входа потребуется QR-код или приглашение.", "You will be signed out from this device. A QR code or invitation will be required to sign in again."]);
        m.insert("settings.devices.scanQR", ["Отсканируйте QR на новом устройстве", "Scan QR on new device"]);
        m.insert("settings.devices.scanQRDescription", ["Направьте камеру на QR-код, отображаемый на новом устройстве", "Point camera at QR code displayed on new device"]);
        m.insert("settings.devices.cameraPlaceholder", ["Камера недоступна в прототипе", "Camera unavailable in prototype"]);
        m.insert("settings.devices.enterManually", ["Ввести вручную", "Enter manually"]);
        m.insert("settings.devices.provisioningId", ["ID запроса", "Request ID"]);
        m.insert("settings.devices.confirmAdd", ["Подтвердите добавление устройства", "Confirm device addition"]);
        m.insert("settings.devices.confirmAddDescription", ["Вы собираетесь добавить новое устройство с ID:", "You are about to add a new device with ID:"]);

        // Appearance
        m.insert("appearance.theme", ["Тема", "Theme"]);
        m.insert("appearance.theme.system", ["Системная", "System"]);
        m.insert("appearance.theme.light", ["Светлая", "Light"]);
        m.insert("appearance.theme.dark", ["Тёмная", "Dark"]);
        m.insert("appearance.language", ["Язык", "Language"]);
        m.insert("appearance.fontSize", ["Размер шрифта", "Font size"]);
        m.insert("appearance.fontSize.small", ["Маленький", "Small"]);
        m.insert("appearance.fontSize.medium", ["Средний", "Medium"]);
        m.insert("appearance.fontSize.large", ["Большой", "Large"]);
        m.insert("settings.appearance.description", ["Настройка внешнего вида приложения", "Customize app appearance"]);

        // Notifications
        m.insert("notifications.enable", ["Включить уведомления", "Enable notifications"]);
        m.insert("notifications.sound", ["Звук", "Sound"]);
        m.insert("notifications.preview", ["Превью сообщений", "Message preview"]);
        m.insert("notifications.readReceipts", ["Отправлять отметки о прочтении", "Send read receipts"]);
        m.insert("notifications.readReceipts.hint", ["Если выключено, отправители не узнают что вы прочитали их сообщения", "If disabled, senders won't know you read their messages"]);
        m.insert("settings.notifications.description", ["Настройка уведомлений о сообщениях", "Configure message notifications"]);
        m.insert("settings.notifications.enableDescription", ["Получать уведомления о новых сообщениях", "Receive notifications for new messages"]);
        m.insert("settings.notifications.soundDescription", ["Воспроизводить звук при получении сообщений", "Play sound when receiving messages"]);
        m.insert("settings.notifications.previewDescription", ["Показывать содержимое сообщений в уведомлениях", "Show message content in notifications"]);
        m.insert("settings.notifications.readReceiptsDescription", ["Если выключено, отправители не узнают что вы прочитали их сообщения", "If disabled, senders will not know you read their messages"]);

        // Privacy
        m.insert("privacy.historyRetention", ["Время хранения локальной истории", "Local history retention"]);
        m.insert("privacy.autoDelete", ["Авто-удаление сообщений в новых чатах", "Auto-delete messages in new chats"]);
        m.insert("privacy.1week", ["1 неделя", "1 week"]);
        m.insert("privacy.1month", ["1 месяц", "1 month"]);
        m.insert("privacy.3months", ["3 месяца", "3 months"]);
        m.insert("privacy.1year", ["1 год", "1 year"]);
        m.insert("privacy.forever", ["Навсегда", "Forever"]);
        m.insert("privacy.off", ["Выкл", "Off"]);
        m.insert("privacy.1day", ["1 день", "1 day"]);
        m.insert("settings.privacy.description", ["Настройки конфиденциальности и хранения данных", "Privacy and data storage settings"]);
        m.insert("settings.privacy.historyRetentionDescription", ["Как долго хранить сообщения на этом устройстве", "How long to keep messages on this device"]);
        m.insert("settings.privacy.autoDeleteDescription", ["Автоматическое удаление сообщений в новых чатах", "Auto-delete messages in new chats"]);

        // Admin invites
        m.insert("admin.invites", ["Приглашения", "Invitations"]);
        m.insert("admin.invites.create", ["Создать приглашение", "Create invitation"]);
        m.insert("admin.invites.role", ["Роль", "Role"]);
        m.insert("admin.invites.maxUses", ["Макс. использований", "Max uses"]);
        m.insert("admin.invites.expiry", ["Время действия", "Expiry"]);
        m.insert("admin.invites.expiry.1h", ["1 час", "1 hour"]);
        m.insert("admin.invites.expiry.1d", ["1 день", "1 day"]);
        m.insert("admin.invites.expiry.1w", ["1 неделя", "1 week"]);
        m.insert("admin.invites.expiry.1m", ["1 месяц", "1 month"]);
        m.insert("admin.invites.expiry.never", ["Без срока", "No expiry"]);
        m.insert("admin.invites.created", ["Приглашение создано", "Invitation created"]);
        m.insert("admin.invites.copy", ["Копировать", "Copy"]);
        m.insert("admin.invites.share", ["Поделиться ссылкой", "Share link"]);
        m.insert("admin.invites.showQr", ["Показать QR", "Show QR"]);
        m.insert("admin.invites.onceWarning", ["Этот токен показывается один раз. Сохраните его сейчас.", "This token is shown only once. Save it now."]);
        m.insert("admin.invites.revoke", ["Отозвать", "Revoke"]);
        m.insert("admin.invites.uses", ["использований", "uses"]);
        m.insert("admin.invites.expiresIn", ["истекает через", "expires in"]);
        m.insert("admin.invites.status", ["Статус", "Status"]);

        // Admin users
        m.insert("admin.users", ["Пользователи", "Users"]);
        m.insert("admin.users.suspend", ["Заблокировать", "Suspend"]);
        m.insert("admin.users.unsuspend", ["Разблокировать", "Unsuspend"]);

        // Admin stats
        m.insert("admin.stats", ["Статистика сервера", "Server statistics"]);
        m.insert("admin.stats.users", ["Пользователей", "Users"]);
        m.insert("admin.stats.activeDay", ["Активных за 24ч", "Active in 24h"]);
        m.insert("admin.stats.groups", ["Групп", "Groups"]);
        m.insert("admin.stats.messagesDay", ["Сообщений за сутки", "Messages today"]);

        // About
        m.insert("about.version", ["Версия", "Version"]);
        m.insert("about.server", ["Адрес сервера", "Server address"]);
        m.insert("about.license", ["Лицензия", "License"]);

        // Profile
        m.insert("profile.title", ["Профиль", "Profile"]);
        m.insert("profile.participants", ["участников", "participants"]);
        m.insert("profile.block", ["Заблокировать", "Block"]);
        m.insert("profile.addMembers", ["Добавить участников", "Add members"]);

        // Errors and states
        m.insert("error.title", ["Ошибка", "Error"]);
        m.insert("error.retry", ["Повторить", "Retry"]);
        m.insert("error.offline", ["Нет соединения с сервером. Сообщения будут отправлены при восстановлении.", "No connection to server. Messages will be sent when connection is restored."]);
        m.insert("loading", ["Загрузка...", "Loading..."]);
        m.insert("cancel", ["Отмена", "Cancel"]);
        m.insert("confirm", ["Подтвердить", "Confirm"]);
        m.insert("back", ["Назад", "Back"]);
        m.insert("close", ["Закрыть", "Close"]);

        // Time
        m.insert("time.today", ["Сегодня", "Today"]);
        m.insert("time.yesterday", ["Вчера", "Yesterday"]);
        m.insert("time.daysAgo", ["дней назад", "days ago"]);
        m.insert("time.seconds", ["сек", "s"]);
        m.insert("time.minutes", ["мин", "min"]);
        m.insert("time.hours", ["ч", "h"]);
        m.insert("time.days", ["дн", "d"]);

        // Scan
        m.insert("scan.title", ["Отсканируйте QR на новом устройстве", "Scan QR on new device"]);
        m.insert("scan.manual", ["Ввести вручную", "Enter manually"]);
        m.insert("scan.confirm.title", ["Подтвердите добавление устройства", "Confirm device addition"]);
        m.insert("scan.confirm", ["Подтвердить", "Confirm"]);

        // Missing keys added by C02 follow-up
        m.insert("chat.direct", ["Личный", "Direct"]);
        m.insert("chat.members", ["участников", "members"]);
        m.insert("chat.pinChat", ["Закрепить чат", "Pin chat"]);
        m.insert("chat.unpinChat", ["Открепить чат", "Unpin chat"]);
        m.insert("settings.about.copyright", ["© 2024 Secure Messenger. Все права защищены.", "© 2024 Secure Messenger. All rights reserved."]);
        m.insert("settings.about.description", ["Информация о приложении и сервере", "App and server information"]);
        m.insert("settings.about.license", ["AGPL-3.0", "AGPL-3.0"]);
        m.insert("settings.about.serverAddress", ["Адрес сервера", "Server address"]);
        m.insert("settings.about.serverName", ["Название сервера", "Server name"]);
        m.insert("settings.about.title", ["О приложении", "About"]);
        m.insert("settings.about.version", ["Версия", "Version"]);
        m.insert("settings.account.bio", ["О себе", "Bio"]);
        m.insert("settings.account.changeAvatar", ["Изменить аватар", "Change avatar"]);
        m.insert("settings.account.displayName", ["Отображаемое имя", "Display name"]);
        m.insert("settings.account.safetyHint", ["Сверьте safety number при личной встрече", "Compare safety number in person"]);
        m.insert("settings.account.safetyNumber", ["Safety Number", "Safety Number"]);
        m.insert("settings.account.save", ["Сохранить", "Save"]);
        m.insert("settings.account.title", ["Аккаунт", "Account"]);
        m.insert("settings.account.username", ["Имя пользователя", "Username"]);
        m.insert("settings.account.usernameHint", ["Username нельзя изменить", "Username cannot be changed"]);
        m.insert("settings.adminInvites.actions", ["Действия", "Actions"]);
        m.insert("settings.adminInvites.cancel", ["Отмена", "Cancel"]);
        m.insert("settings.adminInvites.create", ["Создать приглашение", "Create invitation"]);
        m.insert("settings.adminInvites.createDesc", ["Создать новое приглашение для пользователя", "Create a new user invitation"]);
        m.insert("settings.adminInvites.createTitle", ["Новое приглашение", "New invitation"]);
        m.insert("settings.adminInvites.description", ["Управление приглашениями", "Manage invitations"]);
        m.insert("settings.adminInvites.expires", ["Истекает", "Expires"]);
        m.insert("settings.adminInvites.maxUses", ["Макс. использований", "Max uses"]);
        m.insert("settings.adminInvites.revoke", ["Отозвать", "Revoke"]);
        m.insert("settings.adminInvites.role", ["Роль", "Role"]);
        m.insert("settings.adminInvites.roleAdmin", ["Администратор", "Admin"]);
        m.insert("settings.adminInvites.roleUser", ["Пользователь", "User"]);
        m.insert("settings.adminInvites.status", ["Статус", "Status"]);
        m.insert("settings.adminInvites.title", ["Приглашения", "Invitations"]);
        m.insert("settings.adminInvites.token", ["Токен", "Token"]);
        m.insert("settings.adminInvites.uses", ["Использований", "Uses"]);
        m.insert("settings.adminUsers.actions", ["Действия", "Actions"]);
        m.insert("settings.adminUsers.created", ["Создан", "Created"]);
        m.insert("settings.adminUsers.description", ["Управление пользователями", "Manage users"]);
        m.insert("settings.adminUsers.lastActive", ["Последняя активность", "Last active"]);
        m.insert("settings.adminUsers.role", ["Роль", "Role"]);
        m.insert("settings.adminUsers.status", ["Статус", "Status"]);
        m.insert("settings.adminUsers.suspend", ["Заблокировать", "Suspend"]);
        m.insert("settings.adminUsers.title", ["Пользователи", "Users"]);
        m.insert("settings.adminUsers.unsuspend", ["Разблокировать", "Unsuspend"]);
        m.insert("settings.adminUsers.user", ["Пользователь", "User"]);
        m.insert("settings.appearance.fontLarge", ["Большой", "Large"]);
        m.insert("settings.appearance.fontMedium", ["Средний", "Medium"]);
        m.insert("settings.appearance.fontSize", ["Размер шрифта", "Font size"]);
        m.insert("settings.appearance.fontSmall", ["Маленький", "Small"]);
        m.insert("settings.appearance.langEn", ["English", "English"]);
        m.insert("settings.appearance.langRu", ["Русский", "Russian"]);
        m.insert("settings.appearance.language", ["Язык", "Language"]);
        m.insert("settings.appearance.theme", ["Тема", "Theme"]);
        m.insert("settings.appearance.title", ["Внешний вид", "Appearance"]);
        m.insert("settings.devices.addDevice", ["Добавить устройство", "Add device"]);
        m.insert("settings.devices.added", ["Добавлено", "Added"]);
        m.insert("settings.devices.currentDevice", ["Текущее устройство", "Current device"]);
        m.insert("settings.devices.lastActive", ["Последняя активность", "Last active"]);
        m.insert("settings.devices.newDevice", ["Новое устройство", "New device"]);
        m.insert("settings.devices.revoke", ["Отозвать", "Revoke"]);
        m.insert("settings.devices.title", ["Устройства", "Devices"]);
        m.insert("settings.notifications.enable", ["Включить уведомления", "Enable notifications"]);
        m.insert("settings.notifications.enableDesc", ["Получать уведомления", "Receive notifications"]);
        m.insert("settings.notifications.preview", ["Превью сообщений", "Message preview"]);
        m.insert("settings.notifications.previewDesc", ["Показывать содержимое", "Show content"]);
        m.insert("settings.notifications.readReceipts", ["Отметки о прочтении", "Read receipts"]);
        m.insert("settings.notifications.readReceiptsDesc", ["Отправлять read receipts", "Send read receipts"]);
        m.insert("settings.notifications.sound", ["Звук", "Sound"]);
        m.insert("settings.notifications.soundDesc", ["Звуковое уведомление", "Sound notification"]);
        m.insert("settings.notifications.title", ["Уведомления", "Notifications"]);
        m.insert("settings.privacy.autoDelete", ["Авто-удаление", "Auto-delete"]);
        m.insert("settings.privacy.autoDelete24h", ["24 часа", "24 hours"]);
        m.insert("settings.privacy.autoDelete30d", ["30 дней", "30 days"]);
        m.insert("settings.privacy.autoDelete7d", ["7 дней", "7 days"]);
        m.insert("settings.privacy.autoDelete90d", ["90 дней", "90 days"]);
        m.insert("settings.privacy.autoDeleteHint", ["Автоматическое удаление сообщений", "Auto-delete messages"]);
        m.insert("settings.privacy.autoDeleteOff", ["Выкл", "Off"]);
        m.insert("settings.privacy.historyForever", ["Навсегда", "Forever"]);
        m.insert("settings.privacy.historyHint", ["Как долго хранить сообщения", "How long to keep messages"]);
        m.insert("settings.privacy.historyMonth", ["1 месяц", "1 month"]);
        m.insert("settings.privacy.historyRetention", ["Хранение истории", "History retention"]);
        m.insert("settings.privacy.historyWeek", ["1 неделя", "1 week"]);
        m.insert("settings.privacy.historyYear", ["1 год", "1 year"]);
        m.insert("settings.privacy.title", ["Приватность", "Privacy"]);
        m.insert("settings.voice.description", ["Настройки голосовых сообщений", "Voice message settings"]);
        m.insert("settings.voice.title", ["Голосовые сообщения", "Voice"]);
        m.insert("sidebar.chatList.archive", ["Архив", "Archive"]);
        m.insert("sidebar.chatList.clearHistory", ["Очистить историю", "Clear history"]);
        m.insert("sidebar.chatList.deleteChat", ["Удалить чат", "Delete chat"]);
        m.insert("sidebar.chatList.leaveGroup", ["Покинуть группу", "Leave group"]);
        m.insert("sidebar.chatList.logout", ["Выйти", "Sign out"]);
        m.insert("sidebar.chatList.markRead", ["Прочитать", "Mark read"]);
        m.insert("sidebar.chatList.mute", ["Без звука", "Mute"]);
        m.insert("sidebar.chatList.newChat", ["Новый чат", "New chat"]);
        m.insert("sidebar.chatList.noChats", ["Нет чатов", "No chats"]);
        m.insert("sidebar.chatList.pin", ["Закрепить", "Pin"]);
        m.insert("sidebar.chatList.search", ["Поиск...", "Search..."]);
        m.insert("sidebar.chatList.settings", ["Настройки", "Settings"]);
        m.insert("sidebar.chatList.unmute", ["Включить звук", "Unmute"]);
        m.insert("sidebar.chatList.unpin", ["Открепить", "Unpin"]);

        m
    });

/// Translate a key into the given language.
pub fn t(lang: Language, key: &'static str) -> &'static str {
    let idx = match lang {
        Language::Ru => 0,
        Language::En => 1,
    };
    TRANSLATIONS
        .get(key)
        .map(|a| a[idx])
        .unwrap_or(key)
}

/// Format a timestamp into a short time string (HH:MM).
pub fn format_time(timestamp_ms: f64, lang: Language) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp_ms));
    let h = date.get_hours();
    let m = date.get_minutes();
    format!("{:02}:{:02}", h, m)
}

/// Format a duration in seconds to "M:SS".
pub fn format_duration(seconds: u32) -> String {
    let m = seconds / 60;
    let s = seconds % 60;
    format!("{}:{:02}", m, s)
}

/// Format a date to readable form.
pub fn format_date(timestamp_ms: f64, lang: Language) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp_ms));
    let now = js_sys::Date::new_0();
    let today_start = {
        let d = js_sys::Date::new_0();
        d.set_hours(0);
        d.set_minutes(0);
        d.set_seconds(0);
        d.set_milliseconds(0);
        d.get_time()
    };
    let yesterday_start = today_start - 86_400_000.0;
    let date_time = date.get_time();

    if date_time >= today_start {
        t(lang, "time.today").to_string()
    } else if date_time >= yesterday_start {
        t(lang, "time.yesterday").to_string()
    } else {
        let months_ru = [
            "января", "февраля", "марта", "апреля", "мая", "июня",
            "июля", "августа", "сентября", "октября", "ноября", "декабря",
        ];
        let months_en = [
            "January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December",
        ];
        let month = (date.get_month()) as usize;
        let day = date.get_date();
        match lang {
            Language::Ru => format!("{} {}", day, months_ru[month]),
            Language::En => format!("{} {}", months_en[month], day),
        }
    }
}
