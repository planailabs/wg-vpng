# wg-vpng — WireGuard VPN generator

Self-service WireGuard access to the company network. Users sign in with OIDC
and generate a per-user WireGuard configuration; regenerating a config replaces
that peer's private key (invalidating the old one). The actual WireGuard state
is applied through a pluggable backend.

## Features

- **OIDC login** via [`plan-ai-auth`] (same as mac-mgmt / web-agency).
- **Per-user configs** — generate, show, download, regenerate, delete.
- **Pluggable backends** (`[wireguard] backend = …`):
  - `self-managed` — a kernel WireGuard interface driven by `wg` + `ip`.
  - `network-manager` — an `nmcli` keyfile connection profile.
  - `mikrotik` — RouterOS 7 REST API (via the `mikrotik-api` crate).
- **Dioxus fullstack** UI using the shared `plan-ai-design` system; error and
  login pages rendered with `plan-ai-html`.
- **REST + MCP API** via [`plan-ai-api-mcp`] at `/api/v1/*` and `/mcp`
  (OpenAPI + Swagger UI at `/api/v1/docs`), Bearer-token authenticated.

## Layout

```
server/          the fullstack app (UI + API + backends)
  src/backend/   WireguardBackend trait + self-managed / NM / mikrotik impls
  src/wg.rs      keygen, client-config rendering, address allocation
  src/store.rs   DB layer + backend reconcile
  src/api_mcp/   plan-ai-api-mcp registry + endpoints + token auth
  src/web/       Dioxus app, server functions, OIDC resolver
  migrations/    sqlx migrations
mikrotik-api/    standalone RouterOS REST client
common/, design/ git submodules (../common, ../design)
flake.nix        devshell, package, NixOS module, VM integration test
```

## Develop

Everything runs through the Nix devshell (Rust toolchain, dioxus-cli, tailwind,
postgres, wireguard-tools):

```sh
nix develop
cargo test                                   # unit + backend tests
dx serve --package wg-vpng-server            # run locally (needs a postgres + config.toml)
nix build .#checks.x86_64-linux.integration -L   # NixOS VM end-to-end test
nix build .#wg-vpng-server                   # the production build
```

Copy `server/config.example.toml` to `config.toml`. In debug builds,
`DEV_ONLY_NO_AUTH=1` bypasses OIDC (everyone is `dev@localhost`, admin).

## Deploy (NixOS)

```nix
{
  imports = [ inputs.wg-vpng.nixosModules.default ];
  services.wg-vpng-server = {
    enable = true;
    openFirewall = true;
    settings = {
      wireguard = {
        backend = "self-managed";
        address = "10.8.0.1/24";
        endpoint = "vpn.example.com:51820";
      };
      auth = { /* … OIDC providers … */ };
    };
    environmentFile = "/run/secrets/wg-vpng.env";
  };
}
```

[`plan-ai-auth`]: ../common/plan-ai-auth
[`plan-ai-api-mcp`]: ../common/plan-ai-api-mcp
