{
  description = "wg-vpng — WireGuard VPN generator (OIDC, per-user configs, pluggable backends)";

  inputs = {
    # Include git submodules (common, design) in the flake source.
    self.submodules = true;

    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    {
      overlays.default = import ./overlay.nix { gitSha = self.rev or self.dirtyRev or "unknown"; };
      nixosModules.default = import ./server/module.nix;
      nixosModules.wg-vpng = import ./server/module.nix;
    } //
    flake-utils.lib.eachDefaultSystem (system:
      let
        gitSha = self.rev or self.dirtyRev or "unknown";
        overlays = [
          (import rust-overlay)
          (import ./overlay.nix { inherit gitSha; })
        ];
        pkgs = import nixpkgs { inherit system overlays; };
        toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
          targets = [ "wasm32-unknown-unknown" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            toolchain
            cargo-edit
            cargo-watch

            # Dioxus CLI patched for --embed / --skip-platform-features.
            dioxus-cli-patched

            # Build dependencies
            pkg-config
            openssl
            nodejs
            tailwindcss_3

            # Ephemeral test databases (pgtemp spawns initdb/postgres) + client
            postgresql

            # Local dev process manager (runs the Procfile: server + tailwind)
            overmind

            # WireGuard tooling used by the self-managed + NetworkManager backends
            wireguard-tools
            iproute2

            # For WASM
            wasm-bindgen-cli_0_2_121
            binaryen # wasm-opt
            lld
          ];

          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
        };

        packages = {
          default = pkgs.wg-vpng-server;
          wg-vpng-server = pkgs.wg-vpng-server;
          dioxus-cli-patched = pkgs.dioxus-cli-patched;
        };

        # nixpkgs.lib (not pkgs.lib): evaluating pkgs for unsupported systems
        # throws, and `nix flake show` walks every system.
        checks = nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
          # NixOS VM test: postgres + wg-vpng with the self-managed WireGuard
          # backend; exercises interface + peer creation end to end.
          integration = pkgs.callPackage ./tests/integration.nix { };
        };
      });
}
