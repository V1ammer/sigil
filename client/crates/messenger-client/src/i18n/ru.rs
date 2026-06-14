//! Russian translation dictionary.

use std::collections::HashMap;

/// Build the Russian translation map.
#[must_use]
pub fn ru_dict() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("app.name", "Secure Messenger");
    m.insert("app.description", "Корпоративный E2E мессенджер");

    // Connection screen
    m.insert("connect.title", "Подключение к серверу");
    m.insert("connect.placeholder", "https://messenger.company.com");
    m.insert("connect.button", "Подключиться");
    m.insert("connect.whatsThis", "Что это?");
    m.insert("connect.help.title", "Что такое адрес сервера?");
    m.insert("connect.help.description", "Это адрес корпоративного сервера мессенджера, предоставленный вашим администратором. Все сообщения зашифрованы и хранятся только на этом сервере.");
    m.insert("connect.error.unavailable", "Сервер недоступен");
    m.insert("connect.error.invalid", "Неверный адрес сервера");

    // Login screen
    m.insert("login.title", "Войти на");
    m.insert("login.token.title", "Войти по токену");
    m.insert("login.token.description", "Получили приглашение от админа");
    m.insert("login.qr.title", "Войти с другого устройства");
    m.insert("login.qr.description", "Покажите QR со старого устройства");
    m.insert("login.newDevice", "Это новое устройство существующего аккаунта? Используйте QR.");

    // Token login
    m.insert("token.title", "Введите токен приглашения");
    m.insert("token.placeholder", "XXXX-XXXX-XXXX-XXXX");
    m.insert("token.hint", "Токен предоставил администратор сервера");
    m.insert("token.continue", "Продолжить");
    m.insert("token.error.invalid", "Неверный токен приглашения");
    m.insert("token.error.expired", "Срок действия токена истёк");
    m.insert("token.error.exhausted", "Токен уже использован максимальное число раз");

    // Invite deep-link
    m.insert("invite.connecting", "Подключаемся к серверу…");
    m.insert("invite.failed", "Не удалось перейти по приглашению");
    m.insert("invite.back", "Назад");

    // QR login
    m.insert("qr.title", "Отсканируйте QR код");
    m.insert("qr.instruction", "Откройте мессенджер на устройстве где вы уже вошли, нажмите Настройки → Устройства → Добавить устройство, и отсканируйте этот код.");
    m.insert("qr.validFor", "Действителен ещё");
    m.insert("qr.refresh", "Обновить QR");
    m.insert("qr.create", "Создать QR код");
    m.insert("qr.requestId", "ID запроса");
    m.insert("qr.waiting", "Ждём подтверждения...");
    m.insert("qr.success", "Устройство добавлено");

    // Registration
    m.insert("register.title", "Создание аккаунта");
    m.insert("register.username", "Имя пользователя");
    m.insert("register.username.hint", "Только латинские буквы, цифры и подчёркивание");
    m.insert("register.displayName", "Отображаемое имя");
    m.insert("register.avatar", "Аватар");
    m.insert("register.avatar.hint", "Перетащите изображение или нажмите для выбора");
    m.insert("register.create", "Создать аккаунт");
    m.insert("register.privacy", "Эти данные хранятся зашифрованно. Сервер видит только хэш username.");
    m.insert("register.success", "Аккаунт создан");
    m.insert("register.username.taken", "Это имя пользователя уже занято");
    m.insert("register.username.available", "Имя пользователя доступно");

    // Chats
    m.insert("chats.title", "Чаты");
    m.insert("chats.search", "Поиск...");
    m.insert("chats.search.hint", "Введите @username для поиска пользователя");
    m.insert("chats.empty", "Нет чатов");
    m.insert("chats.empty.hint", "Начните первый чат");
    m.insert("chats.new", "Новый чат");
    m.insert("chats.new.direct", "Личный");
    m.insert("chats.new.group", "Групповой");
    m.insert("chats.archive", "Архив");
    m.insert("chats.filter.all", "Все");
    m.insert("chats.filter.unread", "Непрочитанные");
    m.insert("chats.filter.direct", "Личные");
    m.insert("chats.filter.groups", "Группы");

    // Chat actions
    m.insert("chat.pin", "Закрепить");
    m.insert("chat.unpin", "Открепить");
    m.insert("chat.mute", "Без звука");
    m.insert("chat.unmute", "Включить звук");
    m.insert("chat.mute.1h", "1 час");
    m.insert("chat.mute.8h", "8 часов");
    m.insert("chat.mute.1d", "1 день");
    m.insert("chat.mute.forever", "Навсегда");
    m.insert("chat.markRead", "Прочитать");
    m.insert("chat.archiveChat", "Архивировать");
    m.insert("chat.clearHistory", "Очистить историю");
    m.insert("chat.delete", "Удалить чат");
    m.insert("chat.deleteConfirmTitle", "Удалить чат?");
    m.insert("chat.deleteConfirmDesc", "Переписка и все вложения будут удалены безвозвратно, в том числе у собеседника. Это действие нельзя отменить.");
    m.insert("chat.leave", "Покинуть группу");
    m.insert("chat.profile", "Профиль");
    m.insert("chat.search", "Поиск по чату");
    m.insert("chat.pinChat", "Закрепить чат");
    m.insert("chat.unpinChat", "Открепить чат");
    m.insert("chat.direct", "Личный");
    m.insert("chat.members", "участников");

    // Messages
    m.insert("message.placeholder", "Сообщение");
    m.insert("message.reply", "Ответить");
    m.insert("message.replyThread", "Ответить в треде");
    m.insert("message.editing", "Редактирование");
    m.insert("message.recording", "Запись...");
    m.insert("message.emoji", "Эмодзи");
    m.insert("message.image", "Изображение");
    m.insert("message.imageUnavailable", "Изображение недоступно");
    m.insert("message.video", "Видео");
    m.insert("message.playAudio", "Слушать");
    m.insert("message.download", "Скачать");
    m.insert("message.react", "Реакция");
    m.insert("message.edit", "Изменить");
    m.insert("message.copy", "Копировать");
    m.insert("message.forward", "Переслать");
    m.insert("message.save", "Сохранить");
    m.insert("message.pin", "Закрепить");
    m.insert("message.delete", "Удалить");
    m.insert("message.deleteForAll", "Удалить у всех");
    m.insert("message.deleteForMe", "Удалить у меня");
    m.insert("message.deleted", "Сообщение удалено");
    m.insert("message.edited", "изменено");
    m.insert("message.replies", "ответов");
    m.insert("message.transcribe", "Расшифровать");
    m.insert("message.transcribe.unavailable", "Транскрибация недоступна в этом прототипе");

    // Attachments
    m.insert("attach.photo", "Фото");
    m.insert("attach.file", "Файл");
    m.insert("attach.voice", "Голосовое");

    // Welcome state
    m.insert("welcome.title", "Выберите чат");
    m.insert("welcome.hint", "Выберите чат слева или начните новый");

    m.insert("chat.messages.empty", "Нет сообщений");

    // Chat list
    m.insert("chat.list.empty", "Нет диалогов");
    m.insert("chat.list.empty.hint", "Нажми + чтобы начать диалог");
    m.insert("chat.mls.not_ready", "Сообщения недоступны");
    m.insert("chat.mls.hint", "Настройка MLS требуется для просмотра сообщений");

    // Direct chat creation
    m.insert("chat.create_direct.title", "Новый диалог");
    m.insert("chat.typing", "печатает…");
    m.insert("chat.dropToAttach", "Отпустите файл, чтобы прикрепить");
    m.insert("chat.forwardTitle", "Переслать в…");
    m.insert("chat.forwardSearch", "Поиск людей…");
    m.insert("chat.forwardEmpty", "Никого не найдено");
    m.insert("chat.forwardSent", "Сообщение переслано");
    m.insert("chat.forwardFailed", "Не удалось переслать");
    m.insert("share.pickChat", "Выберите чат, чтобы отправить файл");
    m.insert("chat.create_direct.placeholder", "Введите имя пользователя");
    m.insert("chat.create_direct.create", "Создать чат");
    m.insert("chat.create_direct.creating", "Создание...");
    m.insert("chat.create_direct.cancel", "Отмена");
    m.insert("chat.create_direct.empty_username", "Введите имя пользователя");
    m.insert("chat.create_direct.no_api", "API клиент недоступен");
    m.insert("chat.create_direct.userNotFound", "Пользователь с таким именем не найден");
    m.insert("chat.create_direct.self", "Нельзя создать чат с самим собой");
    m.insert("chat.create_direct.noDevices", "У пользователя нет доступных устройств — попросите его зайти в мессенджер");
    m.insert("chat.create_direct.failed", "Не удалось создать чат, попробуйте ещё раз");

    // Security
    m.insert("security.title", "Безопасность");
    m.insert("security.changes", "Изменения безопасности");
    m.insert("security.safetyNumber", "Safety Number");
    m.insert("security.compare", "Сравнить лично");
    m.insert("security.devices", "устройств");
    m.insert("security.newDevice", "Новое устройство");
    m.insert("security.warning", "Безопасность изменилась — у контакта появилось новое устройство. Сверьте safety number.");

    // Settings
    m.insert("settings.title", "Настройки");
    m.insert("settings.back", "Назад");
    m.insert("settings.account", "Аккаунт");
    m.insert("settings.devices", "Устройства");
    m.insert("settings.appearance", "Внешний вид");
    m.insert("settings.notifications", "Уведомления");
    m.insert("settings.privacy", "Приватность");
    m.insert("settings.admin", "Управление сервером");
    m.insert("settings.about", "О приложении");
    m.insert("settings.logout", "Выйти");

    // Account settings
    m.insert("settings.account.displayName", "Отображаемое имя");
    m.insert("settings.account.displayNameHint", "Имя, которое видят собеседники");
    m.insert("settings.account.userId", "ID пользователя");
    m.insert("settings.account.username", "Имя пользователя");
    m.insert("settings.account.changeAvatar", "Изменить аватар");
    m.insert("settings.account.removeAvatar", "Удалить аватар");
    m.insert("settings.account.avatarSaved", "Аватар обновлён");
    m.insert("settings.account.avatarRemoved", "Аватар удалён");
    m.insert("settings.account.bio", "О себе");
    m.insert("settings.account.cancel", "Отмена");
    m.insert("settings.account.confirm", "Подтвердить");
    m.insert("settings.account.saveLocalSuccess", "Профиль сохранён");
    m.insert("settings.account.notAuthenticated", "Вы не авторизованы");
    m.insert("settings.account.changeUsernameTitle", "Изменить имя пользователя");
    m.insert("settings.account.changeUsernameDescription", "Введите новое имя пользователя. Собеседники узнают о смене из ваших сообщений.");
    m.insert("settings.account.newUsernamePlaceholder", "новое_имя");
    m.insert("settings.account.usernameInvalid", "Имя: 3–32 символа, только a–z, 0–9 и _");
    m.insert("settings.account.usernameChangeError", "Не удалось изменить имя пользователя");
    m.insert("settings.account.apiClientError", "Не удалось подключиться к серверу");
    m.insert("settings.account.signOutSection", "Выход");
    m.insert("settings.account.signOut", "Выйти");
    m.insert("settings.account.signOutHint", "Ключи шифрования будут удалены с этого устройства");
    m.insert("settings.account.signOutConfirmTitle", "Выйти из аккаунта?");
    m.insert("settings.account.signOutConfirmDescription", "Локальные данные и ключи шифрования будут удалены с этого устройства. Для повторного входа понадобится приглашение или другое ваше устройство.");
    m.insert("account.displayName", "Отображаемое имя");
    m.insert("account.username", "Имя пользователя");
    m.insert("account.username.hint", "Username нельзя изменить просто так; обратитесь к админу");
    m.insert("account.bio", "О себе");
    m.insert("account.avatar.change", "Изменить аватар");
    m.insert("account.visibility", "Профиль виден только людям с которыми у вас есть общие чаты");
    m.insert("account.save", "Сохранить");
    m.insert("settings.account.description", "Управление профилем и личными данными");
    m.insert("settings.account.bioPlaceholder", "Расскажите о себе...");
    m.insert("settings.account.privacyNote", "Профиль виден только людям с которыми у вас есть общие чаты");

    // Devices
    m.insert("devices.title", "Мои устройства");
    m.insert("devices.current", "Это устройство");
    m.insert("devices.add", "Добавить устройство");
    m.insert("devices.logout", "Выйти на этом устройстве");
    m.insert("devices.lastActive", "Последняя активность");
    m.insert("devices.added", "Добавлено");
    m.insert("settings.devices.description", "Управление подключёнными устройствами");
    m.insert("settings.devices.current", "Текущее устройство");
    m.insert("settings.devices.other", "Другие устройства");
    m.insert("settings.devices.thisDevice", "Это устройство");
    m.insert("settings.devices.logoutConfirm", "Выйти на этом устройстве?");
    m.insert("settings.devices.logoutDescription", "Вы будете отключены от этого устройства. Для повторного входа потребуется QR-код или приглашение.");
    m.insert("settings.devices.scanQR", "Отсканируйте QR на новом устройстве");
    m.insert("settings.devices.scanQRDescription", "Направьте камеру на QR-код, отображаемый на новом устройстве");
    m.insert("settings.devices.cameraPlaceholder", "Камера недоступна в прототипе");
    m.insert("settings.devices.enterManually", "Ввести вручную");
    m.insert("settings.devices.provisioningId", "ID запроса");
    m.insert("settings.devices.confirmAdd", "Подтвердите добавление устройства");
    m.insert("settings.devices.confirmAddDescription", "Вы собираетесь добавить новое устройство с ID:");
    m.insert("settings.devices.addDevice", "Добавить устройство");
    m.insert("settings.devices.added", "Добавлено");
    m.insert("settings.devices.currentDevice", "Текущее устройство");
    m.insert("settings.devices.lastActive", "Последняя активность");
    m.insert("settings.devices.newDevice", "Новое устройство");
    m.insert("settings.devices.noDevices", "Устройства не найдены");
    m.insert("settings.devices.addDeviceFor", "Добавить устройство для пользователя @");
    m.insert("settings.devices.label", "Устройство");
    m.insert("settings.devices.revoked", "Отозвано");
    m.insert("settings.devices.revoke", "Отозвать");
    m.insert("settings.devices.title", "Устройства");

    // Appearance
    m.insert("appearance.theme", "Тема");
    m.insert("appearance.theme.system", "Системная");
    m.insert("appearance.theme.light", "Светлая");
    m.insert("appearance.theme.dark", "Тёмная");
    m.insert("appearance.language", "Язык");
    m.insert("appearance.fontSize", "Размер шрифта");
    m.insert("appearance.fontSize.small", "Маленький");
    m.insert("appearance.fontSize.medium", "Средний");
    m.insert("appearance.fontSize.large", "Большой");
    m.insert("settings.appearance.description", "Настройка внешнего вида приложения");
    m.insert("settings.appearance.fontLarge", "Большой");
    m.insert("settings.appearance.fontMedium", "Средний");
    m.insert("settings.appearance.fontSize", "Размер шрифта");
    m.insert("settings.appearance.fontSmall", "Маленький");
    m.insert("settings.appearance.langEn", "English");
    m.insert("settings.appearance.langRu", "Русский");
    m.insert("settings.appearance.language", "Язык");
    m.insert("settings.appearance.theme", "Тема");
    m.insert("settings.appearance.title", "Внешний вид");

    // Notifications
    m.insert("notifications.enable", "Включить уведомления");
    m.insert("notifications.sound", "Звук");
    m.insert("notifications.preview", "Превью сообщений");
    m.insert("notifications.readReceipts", "Отправлять отметки о прочтении");
    m.insert("notifications.readReceipts.hint", "Если выключено, отправители не узнают что вы прочитали их сообщения");
    m.insert("settings.notifications.description", "Настройка уведомлений о сообщениях");
    m.insert("settings.notifications.enableDescription", "Получать уведомления о новых сообщениях");
    m.insert("settings.notifications.soundDescription", "Воспроизводить звук при получении сообщений");
    m.insert("settings.notifications.previewDescription", "Показывать содержимое сообщений в уведомлениях");
    m.insert("settings.notifications.readReceiptsDescription", "Если выключено, отправители не узнают что вы прочитали их сообщения");
    m.insert("settings.notifications.enable", "Включить уведомления");
    m.insert("settings.notifications.enableDesc", "Получать уведомления");
    m.insert("settings.notifications.preview", "Превью сообщений");
    m.insert("settings.notifications.previewDesc", "Показывать содержимое");
    m.insert("settings.notifications.readReceipts", "Отметки о прочтении");
    m.insert("settings.notifications.readReceiptsDesc", "Отправлять read receipts");
    m.insert("settings.notifications.sound", "Звук");
    m.insert("settings.notifications.soundDesc", "Звуковое уведомление");
    m.insert("settings.notifications.title", "Уведомления");

    // Privacy
    m.insert("privacy.historyRetention", "Время хранения локальной истории");
    m.insert("privacy.autoDelete", "Авто-удаление сообщений в новых чатах");
    m.insert("privacy.1week", "1 неделя");
    m.insert("privacy.1month", "1 месяц");
    m.insert("privacy.3months", "3 месяца");
    m.insert("privacy.1year", "1 год");
    m.insert("privacy.forever", "Навсегда");
    m.insert("privacy.off", "Выкл");
    m.insert("privacy.1day", "1 день");
    m.insert("settings.privacy.description", "Настройки конфиденциальности и хранения данных");
    m.insert("settings.privacy.historyRetentionDescription", "Как долго хранить сообщения на этом устройстве");
    m.insert("settings.privacy.autoDeleteDescription", "Автоматическое удаление сообщений в новых чатах");
    m.insert("settings.privacy.autoDelete", "Авто-удаление");
    m.insert("settings.privacy.autoDelete24h", "24 часа");
    m.insert("settings.privacy.autoDelete30d", "30 дней");
    m.insert("settings.privacy.autoDelete7d", "7 дней");
    m.insert("settings.privacy.autoDelete90d", "90 дней");
    m.insert("settings.privacy.autoDeleteHint", "Автоматическое удаление сообщений");
    m.insert("settings.privacy.autoDeleteOff", "Выкл");
    m.insert("settings.privacy.historyForever", "Навсегда");
    m.insert("settings.privacy.historyHint", "Как долго хранить сообщения");
    m.insert("settings.privacy.historyMonth", "1 месяц");
    m.insert("settings.privacy.historyRetention", "Хранение истории");
    m.insert("settings.privacy.historyWeek", "1 неделя");
    m.insert("settings.privacy.historyYear", "1 год");
    m.insert("settings.privacy.title", "Приватность");
    m.insert("settings.privacy.autoDownloadFiles", "Автозагрузка файлов");
    m.insert("settings.privacy.autoDownloadFilesDesc", "Автоматически скачивать файлы из чатов");
    m.insert("settings.privacy.autoDownloadMaxSize", "Максимальный размер");
    m.insert("message.file.savedToDownloads", "Сохранено в Downloads");
    m.insert("message.file.saveFailed", "Не удалось сохранить");
    m.insert("chat.preview.image", "Фотография");
    m.insert("chat.preview.video", "Видео");
    m.insert("chat.preview.voice", "Голосовое сообщение");
    m.insert("chat.preview.file", "Файл");
    m.insert("settings.voice", "Голосовые сообщения");
    m.insert("settings.voice.title", "Транскрибация голосовых");
    m.insert("settings.voice.description", "Локальная транскрибация моделями Whisper. Аудио не покидает устройство.");
    m.insert("settings.voice.privacyNote", "Модели и расшифровки хранятся только на устройстве. Whisper выпущен под лицензией MIT.");
    m.insert("settings.voice.download", "Скачать");
    m.insert("settings.voice.downloading", "Скачивание…");
    m.insert("settings.voice.downloaded", "Модель скачана");
    m.insert("settings.voice.downloadFailed", "Ошибка скачивания");
    m.insert("settings.voice.delete", "Удалить");
    m.insert("settings.voice.select", "Выбрать");
    m.insert("settings.voice.activeBadge", "Активна");
    m.insert("settings.voice.multilingual", "многоязычная");
    m.insert("settings.voice.englishOnly", "только английский");
    m.insert("settings.voice.unavailableWeb", "Транскрибация недоступна в браузерной версии. Используйте мобильное или десктоп-приложение.");
    m.insert("voice.transcribe", "Транскрибировать");
    m.insert("voice.transcribing", "Транскрибация…");
    m.insert("voice.transcript", "Расшифровка");
    m.insert("voice.noModel", "Выберите модель в настройках голосовых сообщений");
    m.insert("voice.transcribeFailed", "Не удалось транскрибировать");
    m.insert("voice.hide", "Скрыть");

    // Admin invites
    m.insert("admin.invites", "Приглашения");
    m.insert("admin.invites.create", "Создать приглашение");
    m.insert("admin.invites.role", "Роль");
    m.insert("admin.invites.maxUses", "Макс. использований");
    m.insert("admin.invites.expiry", "Время действия");
    m.insert("admin.invites.expiry.1h", "1 час");
    m.insert("admin.invites.expiry.1d", "1 день");
    m.insert("admin.invites.expiry.1w", "1 неделя");
    m.insert("admin.invites.expiry.1m", "1 месяц");
    m.insert("admin.invites.expiry.never", "Без срока");
    m.insert("admin.invites.created", "Приглашение создано");
    m.insert("admin.invites.copy", "Копировать");
    m.insert("admin.invites.share", "Поделиться ссылкой");
    m.insert("admin.invites.showQr", "Показать QR");
    m.insert("admin.invites.onceWarning", "Этот токен показывается один раз. Сохраните его сейчас.");
    m.insert("admin.invites.revoke", "Отозвать");
    m.insert("admin.invites.uses", "использований");
    m.insert("admin.invites.expiresIn", "истекает через");
    m.insert("admin.invites.status", "Статус");
    m.insert("settings.adminInvites.actions", "Действия");
    m.insert("settings.adminInvites.cancel", "Отмена");
    m.insert("settings.adminInvites.create", "Создать приглашение");
    m.insert("settings.adminInvites.createDesc", "Создать новое приглашение для пользователя");
    m.insert("settings.adminInvites.createTitle", "Новое приглашение");
    m.insert("settings.adminInvites.description", "Управление приглашениями");
    m.insert("settings.adminInvites.expires", "Истекает");
    m.insert("settings.adminInvites.maxUses", "Макс. использований");
    m.insert("settings.adminInvites.revoke", "Отозвать");
    m.insert("settings.adminInvites.role", "Роль");
    m.insert("settings.adminInvites.roleAdmin", "Администратор");
    m.insert("settings.adminInvites.roleUser", "Пользователь");
    m.insert("settings.adminInvites.status", "Статус");
    m.insert("settings.adminInvites.statusActive", "Активен");
    m.insert("settings.adminInvites.statusExpired", "Истёк");
    m.insert("settings.adminInvites.statusRevoked", "Отозван");
    m.insert("settings.adminInvites.statusExhausted", "Исчерпан");
    m.insert("settings.adminInvites.title", "Приглашения");
    m.insert("settings.adminInvites.token", "Токен");
    m.insert("settings.adminInvites.uses", "Использований");

    // Admin users
    m.insert("admin.users", "Пользователи");
    m.insert("admin.users.suspend", "Заблокировать");
    m.insert("admin.users.unsuspend", "Разблокировать");
    m.insert("settings.adminUsers.actions", "Действия");
    m.insert("settings.adminUsers.created", "Создан");
    m.insert("settings.adminUsers.description", "Управление пользователями");
    m.insert("settings.adminUsers.lastActive", "Последняя активность");
    m.insert("settings.adminUsers.role", "Роль");
    m.insert("settings.adminUsers.status", "Статус");
    m.insert("settings.adminUsers.suspend", "Заблокировать");
    m.insert("settings.adminUsers.title", "Пользователи");
    m.insert("settings.adminUsers.unsuspend", "Разблокировать");
    m.insert("settings.adminUsers.user", "Пользователь");

    // Admin stats
    m.insert("admin.stats", "Статистика сервера");
    m.insert("admin.stats.users", "Пользователей");
    m.insert("admin.stats.activeDay", "Активных за 24ч");
    m.insert("admin.stats.groups", "Групп");
    m.insert("admin.stats.messagesDay", "Сообщений за сутки");

    // About
    m.insert("about.version", "Версия");
    m.insert("about.server", "Адрес сервера");
    m.insert("about.license", "Лицензия");
    m.insert("settings.about.copyright", "© 2024 Secure Messenger. Все права защищены.");
    m.insert("settings.about.description", "Информация о приложении и сервере");
    m.insert("settings.about.license", "AGPL-3.0");
    m.insert("settings.about.serverAddress", "Адрес сервера");
    m.insert("settings.about.serverName", "Название сервера");
    m.insert("settings.about.title", "О приложении");
    m.insert("settings.about.version", "Версия");

    // Profile
    m.insert("profile.title", "Профиль");
    m.insert("profile.participants", "участников");
    m.insert("profile.block", "Заблокировать");
    m.insert("profile.addMembers", "Добавить участников");

    // Errors and states
    m.insert("error.title", "Ошибка");
    m.insert("error.retry", "Повторить");
    m.insert("error.offline", "Нет соединения с сервером. Сообщения будут отправлены при восстановлении.");
    m.insert("loading", "Загрузка...");
    m.insert("close", "Закрыть");

    // Time
    m.insert("time.now", "только что");
    m.insert("time.today", "Сегодня");
    m.insert("time.yesterday", "Вчера");
    m.insert("time.daysAgo", "дней назад");
    m.insert("time.seconds", "сек");
    m.insert("time.minutes", "мин");
    m.insert("time.hours", "ч");
    m.insert("time.days", "дн");

    // Scan / Provisioning
    m.insert("scan.title", "Отсканируйте QR на новом устройстве");
    m.insert("scan.manual", "Ввести вручную");
    m.insert("scan.placeholder", "Вставьте содержимое QR сюда");
    m.insert("scan.apply", "Применить");
    m.insert("scan.unavailable", "Сканер недоступен. Вставьте QR-код вручную.");
    m.insert("scan.confirm.title", "Подтвердите добавление устройства");
    m.insert("scan.confirm", "Подтвердить");
    m.insert("scan.nonce", "QR-нонс");
    m.insert("scan.cancel", "Отменить");
    m.insert("scan.error.wrongServer", "QR от другого сервера");
    m.insert("scan.error.wrongUser", "QR другого пользователя");
    m.insert("scan.error.expired", "Запрос на добавление просрочен");
    m.insert("scan.error.decryptFailed", "Не удалось расшифровать данные входа");
    m.insert("scan.error.browserOnly", "Добавление устройства доступно только в приложении (не в браузере)");
    m.insert("scan.progress.approve", "Подтверждение...");
    m.insert("scan.progress.groups", "Добавление в группы");
    m.insert("scan.success", "Устройство успешно добавлено");
    m.insert("settings.devices.scan", "Сканировать QR");
    m.insert("settings.devices.manualEntry", "Ввести вручную");

    // Settings sub-sections
    m.insert("settings.account.save", "Сохранить");
    m.insert("settings.account.safetyHint", "Сверьте safety number при личной встрече");
    m.insert("settings.account.safetyNumber", "Safety Number");
    m.insert("settings.account.title", "Аккаунт");
    m.insert("settings.account.usernameHint", "По этому имени вас находят другие пользователи");


    // Sidebar
    m.insert("sidebar.chatList.archive", "Архив");
    m.insert("sidebar.chatList.clearHistory", "Очистить историю");
    m.insert("sidebar.chatList.deleteChat", "Удалить чат");
    m.insert("sidebar.chatList.leaveGroup", "Покинуть группу");
    m.insert("sidebar.chatList.logout", "Выйти");
    m.insert("sidebar.chatList.markRead", "Прочитать");
    m.insert("sidebar.chatList.mute", "Без звука");
    m.insert("sidebar.chatList.newChat", "Новый чат");
    m.insert("sidebar.chatList.noChats", "Нет чатов");
    m.insert("sidebar.chatList.pin", "Закрепить");
    m.insert("sidebar.chatList.search", "Поиск...");
    m.insert("sidebar.chatList.settings", "Настройки");
    m.insert("sidebar.chatList.unmute", "Включить звук");
    m.insert("sidebar.chatList.unpin", "Открепить");

    // Common
    m.insert("common.cancel", "Отмена");
    m.insert("common.save", "Сохранить");
    m.insert("common.delete", "Удалить");
    m.insert("common.close", "Закрыть");
    m.insert("common.confirm", "Подтвердить");

    // Error messages
    m.insert("error.network", "Ошибка сети");
    m.insert("error.invite_invalid", "Токен приглашения недействителен");
    m.insert("error.invite_expired", "Токен приглашения истёк");
    m.insert("error.invite_exhausted", "Токен приглашения уже использован");
    m.insert("error.username_taken", "Имя пользователя уже занято");

    // C11 — Account
    m.insert("settings.account.changeUsername", "Изменить username");
    m.insert("settings.account.changeUsernameDesc", "Введите новый username. Только латинские буквы, цифры и подчёркивание.");
    m.insert("settings.account.usernameTaken", "Это имя пользователя уже занято");
    m.insert("settings.account.usernameChanged", "Имя пользователя изменено");
    m.insert("settings.account.logout", "Выйти");
    m.insert("settings.account.logoutConfirm", "Выйти из аккаунта?");
    m.insert("settings.account.logoutDesc", "Все локальные данные будут удалены. Для входа потребуется токен приглашения.");

    // C11 — Devices
    m.insert("settings.devices.revokeTitle", "Отозвать устройство");
    m.insert("settings.devices.revokeDesc", "Вы уверены, что хотите отозвать это устройство? Оно будет немедленно отключено.");
    m.insert("settings.devices.revoking", "Отзыв устройства...");
    m.insert("settings.devices.revokedToast", "Устройство отозвано");
    m.insert("settings.devices.removingFromGroups", "Удаление из групп...");

    // C11 — Notifications
    m.insert("settings.notifications.vibration", "Вибрация");
    m.insert("settings.notifications.vibrationDesc", "Вибрировать при получении сообщений");
    m.insert("settings.notifications.filter", "Уведомления для");
    m.insert("settings.notifications.filterAll", "Всех");
    m.insert("settings.notifications.filterMentions", "Только упоминания");
    m.insert("settings.notifications.filterNone", "Нет");
    m.insert("settings.notifications.quietHours", "Тихие часы");
    m.insert("settings.notifications.quietHoursDesc", "Не беспокоить в определённое время");
    m.insert("settings.notifications.quietFrom", "С");
    m.insert("settings.notifications.quietTo", "До");

    // C11 — Privacy
    m.insert("settings.privacy.readReceipts", "Отметки о прочтении");
    m.insert("settings.privacy.readReceiptsDesc", "Показывать другим, что вы прочитали их сообщения");
    m.insert("settings.privacy.typingIndicators", "Индикаторы печати");
    m.insert("settings.privacy.typingIndicatorsDesc", "Показывать другим, когда вы печатаете");
    m.insert("settings.privacy.blockList", "Чёрный список");
    m.insert("settings.privacy.blockListDesc", "Заблокированные пользователи");
    m.insert("settings.privacy.clearCache", "Очистить локальный кэш");
    m.insert("settings.privacy.clearCacheDesc", "Удалить все локальные данные и выйти из аккаунта");
    m.insert("settings.privacy.clearCacheConfirm", "Очистить кэш?");
    m.insert("settings.privacy.clearCacheAction", "Очистить");
    m.insert("settings.privacy.clearCacheWarning", "Все локальные данные будут безвозвратно удалены. Для продолжения потребуется повторный вход.");

    // C11 — About
    m.insert("settings.about.mlsVersion", "Версия протокола MLS");
    m.insert("settings.about.serverPubkey", "Публичный ключ сервера");
    m.insert("settings.about.docs", "Документация");
    m.insert("settings.about.source", "Исходный код");
    m.insert("settings.about.bugTracker", "Баги и предложения");
    m.insert("settings.about.licenseDesc", "AGPL-3.0 — свободное программное обеспечение");

    // C11 — Admin
    m.insert("settings.admin.invites", "Приглашения");
    m.insert("settings.admin.users", "Пользователи");
    m.insert("settings.adminInvites.ttl", "Срок действия");
    m.insert("settings.adminInvites.ttl1h", "1 час");
    m.insert("settings.adminInvites.ttl24h", "24 часа");
    m.insert("settings.adminInvites.ttl7d", "7 дней");
    m.insert("settings.adminInvites.ttl30d", "30 дней");
    m.insert("settings.adminInvites.copyToken", "Скопировать токен");
    m.insert("settings.adminInvites.tokenCopied", "Токен скопирован");
    m.insert("settings.adminInvites.onceWarning", "Этот токен показывается один раз. Сохраните его сейчас.");
    m.insert("settings.adminInvites.revokeConfirm", "Отозвать приглашение?");
    m.insert("settings.adminInvites.revokeDesc", "Это приглашение станет недействительным.");
    m.insert("settings.adminInvites.noInvites", "Нет активных приглашений");
    m.insert("settings.adminInvites.created", "Приглашение создано");
    m.insert("settings.adminInvites.createFailed", "Не удалось создать приглашение");
    m.insert("settings.adminUsers.userId", "ID пользователя");
    m.insert("settings.adminUsers.devicesCount", "Устройств");
    m.insert("settings.adminUsers.suspendConfirm", "Заблокировать пользователя?");
    m.insert("settings.adminUsers.suspendDesc", "Пользователь не сможет отправлять сообщения до разблокировки.");
    m.insert("settings.adminUsers.unsuspendConfirm", "Разблокировать пользователя?");
    m.insert("settings.adminUsers.unsuspendDesc", "Пользователь снова сможет пользоваться мессенджером.");
    m.insert("settings.adminUsers.noUsers", "Пользователи не найдены");

    // C11 — Common
    m.insert("common.ok", "OK");
    m.insert("common.yes", "Да");
    m.insert("common.no", "Нет");

    m
}
