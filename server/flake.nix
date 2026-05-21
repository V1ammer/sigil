{
  description = "messenger server";

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
        toolchain = fenix.packages.${system}.stable.toolchain;
      in {
        devShells.default = pkgs.mkShell {
          packages = [ toolchain ] ++ (with pkgs; [
            sea-orm-cli sqlite pkg-config openssl
            cargo-nextest cargo-watch
          ]);

          shellHook = ''
            export DATABASE_URL="sqlite://./dev.db?mode=rwc"
          '';
        };
      });
}
