{
  description = "messenger client (Leptos + Tauri)";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { nixpkgs, flake-utils, fenix, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config = {
            android_sdk.accept_license = true;
            allowUnfree = true;
          };
        };
        toolchain = fenix.packages.${system}.combine [
          fenix.packages.${system}.stable.toolchain
          fenix.packages.${system}.targets.wasm32-unknown-unknown.stable.rust-std
          fenix.packages.${system}.targets.aarch64-linux-android.stable.rust-std
          fenix.packages.${system}.targets.armv7-linux-androideabi.stable.rust-std
          fenix.packages.${system}.targets.x86_64-linux-android.stable.rust-std
          fenix.packages.${system}.targets.i686-linux-android.stable.rust-std
        ];

        android = pkgs.androidenv.composeAndroidPackages {
          platformVersions = [ "36" "35" "34" "33" ];
          buildToolsVersions = [ "35.0.0" "34.0.0" ];
          includeNDK = true;
          ndkVersions = [ "26.1.10909125" ];
        };
      in {
        devShells.default = pkgs.mkShell {
          packages = [ toolchain ] ++ (with pkgs; [
            trunk
            wasm-bindgen-cli
            tailwindcss
            nodejs_20  # tauri-cli npm-packages
            pkg-config
            cmake
            clang
            llvmPackages.libclang
            openssl
            sqlite
            cargo-tauri
            cargo-nextest
            cargo-watch
            # Tauri desktop deps (Linux):
            gtk3
            webkitgtk_4_1
            libsoup_3
            librsvg
            # Android:
            android.androidsdk
            jdk17
            # Tools:
            adb-sync
            android-tools  # adb, fastboot
            chromium       # для chrome-devtools-mcp
          ]);
          shellHook = ''
            export ANDROID_HOME="${android.androidsdk}/libexec/android-sdk"
            export ANDROID_SDK_ROOT="$ANDROID_HOME"
            export NDK_HOME="$ANDROID_HOME/ndk-bundle"
            export ANDROID_NDK_ROOT="$NDK_HOME"
            export JAVA_HOME=${pkgs.jdk17.home}
            export PATH="$ANDROID_HOME/platform-tools:$PATH"
            # Android cross-compilation environment variables
            export CC_aarch64_linux_android="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
            export AR_aarch64_linux_android="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"

            export CC_armv7_linux_androideabi="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang"
            export AR_armv7_linux_androideabi="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$CC_armv7_linux_androideabi"

            export CC_x86_64_linux_android="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"
            export AR_x86_64_linux_android="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$CC_x86_64_linux_android"
            # PKG_CONFIG_PATH is managed automatically by mkShell via nativeBuildInputs

            # whisper-rs build needs LIBCLANG_PATH for bindgen.
            export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          '';
        };
      });
}
