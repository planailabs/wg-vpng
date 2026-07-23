# wg-vpng — agent notes

WireGuard VPN generator. Dioxus fullstack + plan-ai-api-mcp API + OIDC
(plan-ai-auth) + pluggable WireGuard backends. Modeled closely on the sibling
`web-agency` repo — consult it for patterns.

## Build & test (always via the Nix devshell)

```sh
nix develop
cargo test                                        # unit + pgtemp integration tests
cargo check -p wg-vpng-server                     # server-side typecheck
nix build .#checks.x86_64-linux.integration -L    # NixOS VM end-to-end test
nix build .#wg-vpng-server                         # release build (dx fullstack)
```

The release package builds with `dx build --fullstack` and the patched
`dioxus-cli` (see `overlay.nix` / `patches/`). The dioxus fork is pinned via
`[patch.crates-io]` in the root `Cargo.toml` and hashed in `extra-hashes.nix`.

## Conventions

- **Submodules**: `common` and `design` (`../common`, `../design`). Consumed as
  path deps; don't vendor their code here.
- **Interfaces are admin-managed** (DB rows; no interface config in
  config.toml). Each row carries its **own backend** (`BackendConfig`, a JSONB
  column), a per-user `device_limit`, and `access_patterns`. `[wireguard]` no
  longer exists in config — only `[database]`, `[web]`, `[secrets]`, `[auth]`.
- **Backends** live in the shared `wg-backend/` crate (`WireguardBackend` trait
  + `InterfaceSpec`/`PeerSpec`/`PeerStatus` wire types + impls: self-managed,
  network-manager, systemd-networkd, mikrotik, node). `server/src/backend/`
  keeps only `BackendConfig` — the per-interface, secret-encrypted choice.
  Add a backend by implementing `WireguardBackend::{apply, status, remove}`
  (full-state reconcile) in `wg-backend`, then a `BackendConfig` variant +
  `from_parts` + `build`. Keep request/command construction in pure functions
  with unit tests. Backends are built per-interface at sync time
  (`store::sync_interface`), not globally.
- **The `node` backend** drives a remote `wg-vpng-node` (`node/` crate) over
  HTTP; the node is a dumb switch that applies pushed state with its *own* local
  backend (self-managed / systemd-networkd), configured only with a `bind`,
  `api_key`, and `backend`. `wg-backend::local_backend(kind)` builds a node's
  local backend; the node router (`node/src/lib.rs::app`) is bearer-authed.
- **Server addresses and the backend *type* are immutable** after interface
  creation (enforced in `store::update_interface`); MikroTik/node credentials
  can still be edited (blank secret = keep the stored one).
- **Access is pattern-only** (`store::email_matches`): a user can use an
  interface iff their email matches its `access_patterns` (`*` = all; literal
  emails allowed; empty = nobody). No manual grants. `list_active_peers` filters
  by this, so editing patterns + a sync grants/revokes in real time. Any peer
  mutation, pattern edit, or ban/revoke triggers a sync.
- **Secrets** (MikroTik password, node API key) are encrypted at rest
  (`server/src/crypto.rs`, AES-256-GCM) with the key from `[secrets]
  encryption_key`. Never store or log a credential in plaintext; encrypt via
  `BackendConfig::encrypt_secrets`.
- **Interface bring-up failures** are persisted to `wg_interfaces.last_error`
  by `sync_interface` and surfaced in the admin UI.
- **API** = one `plan-ai-api-mcp` `Registry` in `server/src/api_mcp/`; handlers
  are shared by REST + MCP. Mutations require an admin token.
- **Schema changes** go through a new `server/migrations/NNNN_*.sql` — never
  edit an applied migration.
- **UI** uses `plan-ai-design` components + tailwind (tokens from
  `../design/assets/input.css`); `plan-ai-html` renders login + error pages.
- **i18n**: all visible UI text goes through `dioxus_i18n::t!("key")`; keys live
  in `server/src/web/{en-US,de-DE}.ftl` (concatenated with `plan_ai_design::i18n`
  in `App()`). Add a key to *both* ftl files when adding UI text — never hardcode
  strings in components.
- **Auth** is `plan-ai-auth` from the `common` submodule (OIDC); don't handroll.
  The API registry (`api_mcp/`) should expose every operation as REST+MCP.
