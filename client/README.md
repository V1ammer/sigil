# Messenger Client

Cross-platform клиент мессенджера: web (через trunk serve), desktop (Tauri 2), mobile Android (Tauri Mobile).

## Setup
```bash
direnv allow   # или nix develop
```

## Run

### Web dev
```bash
cd crates/messenger-client
trunk serve
# open http://localhost:1420
```

### Desktop dev (Tauri)
```bash
cd src-tauri
cargo tauri dev
```

### Android (требует подключённого устройства)
```bash
# Инициализация (один раз)
cd src-tauri
cargo tauri android init

# Сборка и запуск (с устройством по USB)
cargo tauri android dev --target aarch64

# Или сборка APK без запуска:
cargo tauri android build --debug --target aarch64
```

### Smoke test script
```bash
# Автоматический E2E тест на Android:
./scripts/android-smoke.sh --build
```

## Build

### Web
```bash
cd crates/messenger-client
trunk build --release
```

### Desktop
```bash
cd src-tauri
cargo tauri build
```

### Android APK
```bash
cd src-tauri
cargo tauri android build --debug --target aarch64
# APK: gen/android/app/build/outputs/apk/universal/debug/
```

## Android Keystore Plugin

Для безопасного хранения секретов на Android используется кастомный Tauri-плагин
`tauri-plugin-android-keystore`. Он использует `androidx.security.crypto.EncryptedSharedPreferences`
с `MasterKey` (AES-256 GCM) для аппаратного шифрования.

Плагин автоматически подключается при сборке для Android.

## Tests
```bash
cargo nextest run --workspace
# Web специфика: тесты под wasm32-unknown-unknown
# wasm-pack test --headless --chrome crates/messenger-client
```

## Известные ограничения

### Android
- **Фоновые уведомления (push)**: на mobile приложение не получает уведомления
  в background — требуется реализация FCM (Firebase Cloud Messaging).
  Сейчас используется WebSocket, который работает только в foreground.
- **iOS не поддерживается** — только Android.
- **API level**: `minSdk = 24` (Android 7.0+), targetSdk = 34.
- **WebView рендеринг**: UI рендерится WebView'ем Tauri. Некоторые CSS-фичи
  могут не работать на Android ≤8. Целевой минимум — Android 10 (API 29).
- **Шифрование БД**: на Android используется stock SQLite (bundled) без SQLCipher,
  т.к. сборка SQLCipher требует кросс-компиляции OpenSSL для Android.
  Чувствительные данные защищены через `EncryptedSharedPreferences` (Keystore plugin).
- **FCM**: отложен до пост-MVP. Серверная часть (модели + миграции) готова,
  роуты не реализованы.
- **Биометрия**: `setUserAuthenticationRequired(false)` в Keystore plugin.
  Биометрическая аутентификация не реализована.

### Desktop
- OS keyring (`keyring` crate) для хранения ключей.
- SQLCipher для шифрования локальной БД.
