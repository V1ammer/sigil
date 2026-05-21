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
        pkgs = nixpkgs.legacyPackages.${system};
        toolchain = fenix.packages.${system}.combine [
          fenix.packages.${system}.stable.toolchain
          fenix.packages.${system}.targets.wasm32-unknown-unknown.stable.rust-std
          fenix.packages.${system}.targets.aarch64-linux-android.stable.rust-std
          fenix.packages.${system}.targets.armv7-linux-androideabi.stable.rust-std
          fenix.packages.${system}.targets.x86_64-linux-android.stable.rust-std
          fenix.packages.${system}.targets.i686-linux-android.stable.rust-std
        ];

        android = pkgs.androidenv.composeAndroidPackages {
          platformVersions = [ "34" "33" ];
          buildToolsVersions = [ "34.0.0" ];
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
            # PKG_CONFIG_PATH is managed automatically by mkShell via nativeBuildInputs
          '';
        };
      });
}
