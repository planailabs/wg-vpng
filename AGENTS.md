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
- **Backends** live in `server/src/backend/`. Add one by implementing
  `WireguardBackend::{apply, status}` (full-state reconcile) and a
  `BackendConfig` variant. Keep request/command construction in pure functions
  with unit tests (see `mikrotik-api`, `selfmanaged::render_server_config`).
- **API** = one `plan-ai-api-mcp` `Registry` in `server/src/api_mcp/`; handlers
  are shared by REST + MCP. Mutations require an admin token.
- **Schema changes** go through a new `server/migrations/NNNN_*.sql` — never
  edit an applied migration.
- **UI** uses `plan-ai-design` components + tailwind (tokens from
  `../design/assets/input.css`); `plan-ai-html` renders login + error pages.
