# wg-vpng — WireGuard VPN generator

Self-service WireGuard access to the company network. Users sign in with OIDC
and generate a per-user WireGuard configuration; regenerating a config replaces
that peer's private key (invalidating the old one). The actual WireGuard state
is applied through a pluggable backend.

## Features

- **OIDC login** via [`plan-ai-auth`] (same as mac-mgmt / web-agency).
- **Multiple interfaces, admin-managed** — create/edit/delete WireGuard
  interfaces entirely in the admin UI (`/interfaces`); nothing about interfaces
  lives in config. Each interface has its **own backend**, per-user device
  limit, and access policy.
- **Pattern-based access** — each interface carries access patterns (globs like
  `*@corp.com`, literal emails, or `*` for everyone). A user may use an
  interface iff their email matches. Editing patterns re-syncs immediately, so
  access is granted/revoked in real time (devices dropped from the backend).
- **Per-user devices** — each user generates device configs on any interface
  they can access (show, copy, regenerate, delete), bounded by that interface's
  device limit.
- **Admin console** (`/admin`) — manage any user's devices, revoke VPN access,
  ban (blocks login + drops devices), and delete users.
- **Dual-stack** — IPv6 is always on (non-optional): interfaces are IPv4 + IPv6
  and every device gets an address in each subnet. Admins can also assign a
  device a whole routed subnet of any prefix (e.g. an IPv6 `/64`).
- **Per-interface pluggable backends** (chosen when creating the interface):
  - `self-managed` — a kernel WireGuard interface driven by `wg` + `ip`.
  - `network-manager` — an `nmcli` keyfile connection profile.
  - `systemd-networkd` — a `.netdev`/`.network` pair + `networkctl reload`.
  - `mikrotik` — RouterOS 7 REST API (via the `mikrotik-api` crate); the
    password is encrypted at rest with `[secrets] encryption_key`.
  - `node` — a remote [`wg-vpng-node`](node/) agent driven over HTTP; the
    node applies interfaces with its *own* local backend (self-managed /
    systemd-networkd). The node API key is encrypted at rest.
- **Dioxus fullstack** UI using the shared `plan-ai-design` system; error and
  login pages rendered with `plan-ai-html`.
- **REST + MCP API** via [`plan-ai-api-mcp`] at `/api/v1/*` and `/mcp`
  (OpenAPI + Swagger UI at `/api/v1/docs`), Bearer-token authenticated.

## Layout

```
server/          the fullstack app (UI + API)
  src/backend/   BackendConfig (per-interface choice, secrets encrypted)
  src/wg.rs      keygen, client-config rendering, address allocation
  src/store.rs   DB layer + backend reconcile
  src/api_mcp/   plan-ai-api-mcp registry + endpoints + token auth
  src/web/       Dioxus app, server functions, OIDC resolver
  migrations/    sqlx migrations
wg-backend/      shared WireguardBackend trait + reconcile impls + wire types
                 (self-managed / NM / systemd-networkd / mikrotik / node)
node/            wg-vpng-node — a dumb WireGuard switch driven by the server
mikrotik-api/    standalone RouterOS REST client
common/, design/ git submodules (../common, ../design)
flake.nix        devshell, packages, NixOS modules, per-backend VM tests
```

## Develop

Everything runs through the Nix devshell (Rust toolchain, dioxus-cli, tailwind,
postgres, wireguard-tools):

```sh
nix develop
# unit + backend + pgtemp tests (server feature avoids the wasm build)
cargo test -p wg-vpng-server --no-default-features --features server
cargo test -p wg-backend -p wg-vpng-node -p mikrotik-api
overmind start                               # Procfile: dx serve (DEV_ONLY_NO_AUTH) + tailwind watch
nix build .#checks.x86_64-linux.integration -L   # NixOS VM end-to-end test
nix build .#wg-vpng-server                   # the production build
```

Copy `server/config.example.toml` to `config.toml` (database, web port,
`[secrets] encryption_key`, and optional `[auth]`). In debug builds,
`DEV_ONLY_NO_AUTH=1` bypasses OIDC (everyone is `dev@localhost`, admin). Create
interfaces from the admin UI once running.

CI (GitLab, `nix-image` runner): `.gitlab-ci.yml` warms the xzar cache
(`xzar.sh`), runs `cargo test` + the per-backend VM tests (`nix flake check`),
and pushes the OCI image (`docker-push.sh`). `nix build .#docker` builds the
image; `nix build .#image` builds the NixOS-in-Incus CI runner.

## Deploy (NixOS)

```nix
{
  imports = [ inputs.wg-vpng.nixosModules.default ];
  services.wg-vpng-server = {
    enable = true;
    openFirewall = true;
    settings = {
      secrets.encryption_key = "…"; # openssl rand -base64 32
      auth = { /* … OIDC providers … */ };
    };
    environmentFile = "/run/secrets/wg-vpng.env";
  };
  # Create interfaces (and open their per-interface UDP ports) from the admin UI.
}
```

To offload interfaces to a remote box, run a node there and point a `node`
interface at it (URL + the same key):

```nix
{
  imports = [ inputs.wg-vpng.nixosModules.wg-vpng-node ];
  services.wg-vpng-node = {
    enable = true;
    openFirewall = true;                     # opens the control-API TCP port
    settings.backend = "self-managed";       # or "systemd-networkd"
    apiKeyFile = "/run/secrets/wg-vpng-node.key";  # kept out of the store
  };
}
```

[`plan-ai-auth`]: ../common/plan-ai-auth
[`plan-ai-api-mcp`]: ../common/plan-ai-api-mcp
