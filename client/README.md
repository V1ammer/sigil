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

### Android (после C12, требует подключённого устройства)
```bash
cd src-tauri
cargo tauri android init   # один раз
cargo tauri android dev    # с подключённым устройством по USB
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

## Tests
```bash
cargo nextest run --workspace
# Web специфика: тесты под wasm32-unknown-unknown
# wasm-pack test --headless --chrome crates/messenger-client
```
