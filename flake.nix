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

    # Binary cache client (xzar.plan.ai) + the NixOS-in-Incus CI runner image,
    # same as mac-mgmt / hugger.
    xzar.url = "github:mkg20001/xzar";
    xzar.inputs.nixpkgs.follows = "nixpkgs";
    gitlab-incus-image.url = "git+https://git.mkg20001.io/mkg20001/gitlab-incus-image.git";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, xzar, gitlab-incus-image, ... }:
    {
      overlays.default = import ./overlay.nix { gitSha = self.rev or self.dirtyRev or "unknown"; };
      nixosModules.default = import ./server/module.nix;
      nixosModules.wg-vpng = import ./server/module.nix;
      nixosModules.wg-vpng-node = import ./node/module.nix;
    } //
    flake-utils.lib.eachDefaultSystem (system:
      let
        gitSha = self.rev or self.dirtyRev or "unknown";
        overlays = [
          (import rust-overlay)
          (import ./overlay.nix { inherit gitSha; })
          xzar.overlays.default
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

            # CI: push the OCI image (docker-push.sh) + warm the xzar cache (xzar.sh)
            skopeo
            xzar-client
          ];

          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
        };

        packages = {
          default = pkgs.wg-vpng-server;
          wg-vpng-server = pkgs.wg-vpng-server;
          wg-vpng-node = pkgs.wg-vpng-node;
          dioxus-cli-patched = pkgs.dioxus-cli-patched;
        } // nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
          # OCI image (nix dockerTools). Push with docker-push.sh.
          docker = import ./docker.nix {
            inherit pkgs;
            wg-vpng-server = pkgs.wg-vpng-server;
            tag = gitSha;
          };

          # NixOS-in-Incus image for the GitLab `nix-image` CI runner, wired to
          # the xzar.plan.ai binary cache.
          image = (nixpkgs.lib.nixosSystem {
            system = "x86_64-linux";
            modules = [
              "${nixpkgs}/nixos/modules/virtualisation/lxc-container.nix"
              gitlab-incus-image.nixosModules.gitlab-incus-image
              ({ pkgs, ... }: {
                environment.systemPackages = with pkgs; [ openssh rsync xzar-client pixz ];
                nixpkgs.overlays = [ xzar.overlays.default ];
                programs.git.config.advice.detachedHead = false;
                system.stateVersion = "26.11";
                nix.settings = {
                  substituters = [ "https://xzar.plan.ai" ];
                  trusted-public-keys = [
                    "xzar.plan.ai:KUE66pjr6UX5HHCn9kedN1DJ2J5nSlBrKmE7tUjXewE="
                  ];
                };
              })
            ];
          }).config.system.build.gitlab-incus-image;
        };

        # nixpkgs.lib (not pkgs.lib): evaluating pkgs for unsupported systems
        # throws, and `nix flake show` walks every system.
        #
        # One generic NixOS VM test (tests/lib.nix) instantiated for every
        # backend: self-managed, NetworkManager, systemd-networkd, node (server
        # + a local wg-vpng-node), and MikroTik (against a fake RouterOS REST
        # server). Run e.g.:
        #   nix build .#checks.x86_64-linux.integration-node -L
        checks = nixpkgs.lib.optionalAttrs (system == "x86_64-linux") (
          import ./tests/backends.nix { inherit pkgs; }
        );
      });
}
