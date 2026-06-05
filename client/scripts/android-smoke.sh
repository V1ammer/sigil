#!/usr/bin/env bash
# Android E2E smoke test script.
# Usage: ./scripts/android-smoke.sh [--build]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Android Smoke Test ==="
echo "Project: $PROJECT_DIR"

# 1. Check adb device
echo ""
echo "=== Step 1: Checking adb devices ==="
adb devices | head -20
DEVICES=$(adb devices | grep -v "List" | grep -v "^$" | grep -v "offline" | wc -l)
if [ "$DEVICES" -eq 0 ]; then
    echo "ERROR: No Android device connected via adb"
    echo "Connect a device via USB (or WiFi debugging) and try again."
    echo "  adb tcpip 5555"
    echo "  adb connect <device-ip>:5555"
    exit 1
fi
echo "✅ Device connected"

# 2. Clear logcat
echo ""
echo "=== Step 2: Clearing logcat ==="
adb logcat -c 2>/dev/null || true
echo "✅ Logcat cleared"

# 3. Build (if --build flag)
if [ "${1:-}" = "--build" ]; then
    echo ""
    echo "=== Step 3: Building APK ==="
    cd "$PROJECT_DIR/src-tauri"
    # Set up Android linker env vars
    export CC_aarch64_linux_android="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
    export AR_aarch64_linux_android="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
    cargo tauri android build --debug --target aarch64 2>&1 | tail -20
    echo "✅ Build complete"
fi

# 4. Find APK
echo ""
echo "=== Step 4: Finding APK ==="
APK=$(find "$PROJECT_DIR/src-tauri/gen/android/app/build/outputs/apk" -name "*-debug.apk" 2>/dev/null | head -1)
if [ -z "$APK" ]; then
    echo "ERROR: No APK found. Run with --build flag first."
    exit 1
fi
echo "APK: $APK"
echo "✅ APK found"

# 5. Uninstall old version
echo ""
echo "=== Step 5: Uninstalling old version ==="
adb uninstall com.example.messenger 2>/dev/null || true
echo "✅ Old version removed"

# 6. Install
echo ""
echo "=== Step 6: Installing APK ==="
adb install -r "$APK" 2>&1
echo "✅ APK installed"

# 7. Launch app
echo ""
echo "=== Step 7: Launching app ==="
adb shell am start -n com.example.messenger/com.example.messenger.MainActivity 2>&1
echo "✅ App launched"

# 8. Wait and screenshot
echo ""
echo "=== Step 8: Taking screenshot ==="
sleep 5
adb shell screencap -p /sdcard/screen.png
adb pull /sdcard/screen.png /tmp/messenger-smoke-screen.png 2>&1
echo "✅ Screenshot saved to /tmp/messenger-smoke-screen.png"

# 9. Dump UI hierarchy
echo ""
echo "=== Step 9: UI hierarchy dump ==="
adb shell uiautomator dump /sdcard/window.xml 2>/dev/null || echo "(uiautomator may not be available)"
adb pull /sdcard/window.xml /tmp/messenger-smoke-ui.xml 2>/dev/null || true
echo "✅ UI dump: /tmp/messenger-smoke-ui.xml"

# 10. Check logcat for crashes
echo ""
echo "=== Step 10: Checking logcat for crashes ==="
adb logcat -d -s "RustStdoutStderr:V" "tauri:V" "messenger:V" "AndroidRuntime:V" 2>/dev/null | tail -30 || true

echo ""
echo "=== Smoke test complete ==="
echo "Screenshot: /tmp/messenger-smoke-screen.png"
echo "UI dump:    /tmp/messenger-smoke-ui.xml"
