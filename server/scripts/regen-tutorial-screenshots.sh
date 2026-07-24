#!/usr/bin/env bash
# Regenerate the language-specific setup-guide screenshots, fully self-contained:
# boots a throwaway Postgres (à la pgtemp, via initdb/pg_ctl), runs the built app
# against it, seeds a "plan.ai" interface with plan.ai-looking values, drives the
# UI over CDP in both languages, then tears everything down.
#
#   nix develop --command server/scripts/regen-tutorial-screenshots.sh
#
# Needs: the devshell (postgres, google-chrome, dioxus-cli), passwordless sudo
# (the self-managed backend brings a real WireGuard interface up, like `overmind`
# does in dev). Regenerates app-{create,config}-{desktop,mobile}-{en,de}.png only;
# the generic WireGuard-client screenshots are hand-captured and left untouched.
set -euo pipefail

cd "$(dirname "$0")/.."                 # -> server/
APP_PORT=${APP_PORT:-8091}
PG_PORT=${PG_PORT:-54329}
IFACE=planai                            # Linux wg interface name (display name is plan.ai)
TMP=$(mktemp -d)
SOCK="$TMP/sock"; PGDATA="$TMP/pg"
mkdir -p "$SOCK"

srv_pid=""
restart_overmind=""
cleanup() {
  [ -n "$srv_pid" ] && sudo kill "$srv_pid" 2>/dev/null || true
  sudo ip link del "$IFACE" 2>/dev/null || true
  pg_ctl -D "$PGDATA" -m immediate stop >/dev/null 2>&1 || true
  [ -n "$restart_overmind" ] && OVERMIND_SOCKET="$OVERMIND_SOCK" overmind restart server 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT

# A debug build (needed for DEV_ONLY_NO_AUTH) carries dioxus's hot-reload client,
# which reflects the live `dx serve` (overmind) rebuild state and would show a
# "being rebuilt" overlay mid-capture. Pause that dev server for the run. The
# overmind socket lives in the repo root (one up from server/).
OVERMIND_SOCK="$(cd .. && pwd)/.overmind.sock"
if [ -S "$OVERMIND_SOCK" ]; then
  echo "==> pausing the live dev server (overmind) during capture"
  OVERMIND_SOCKET="$OVERMIND_SOCK" overmind stop server 2>/dev/null && restart_overmind=1 || true
fi

echo "==> ephemeral postgres"
initdb -D "$PGDATA" -U postgres --auth=trust >/dev/null
pg_ctl -D "$PGDATA" -w -o "-k $SOCK -p $PG_PORT -c listen_addresses=''" start >/dev/null
createdb -h "$SOCK" -p "$PG_PORT" -U postgres wgshots
DBURL="postgres:///wgshots?host=$SOCK&port=$PG_PORT&user=postgres"

echo "==> config"
cat > "$TMP/config.toml" <<EOF
[database]
url = "$DBURL"
[web]
port = $APP_PORT
EOF

echo "==> building an isolated debug app bundle"
# Debug (not release) so DEV_ONLY_NO_AUTH works — release strips that path. A
# dedicated target dir keeps it off the live dx-serve bundle; overmind is paused
# above so its rebuilds don't pollute the hot-reload client. Slow the first
# time, cached after.
export CARGO_TARGET_DIR="${SHOTS_TARGET_DIR:-/tmp/wg-vpng-shots}"
dx build --fullstack --package wg-vpng-server
WEB="$CARGO_TARGET_DIR/dx/wg-vpng-server/debug/web"
[ -x "$WEB/server" ] || { echo "build did not produce $WEB/server"; exit 1; }
echo "    using $WEB"

echo "==> starting app on :$APP_PORT (root, for the self-managed backend)"
# Impersonate a realistic, non-admin end user so the screenshots show the
# member's view (no admin tabs) with a proper name/email instead of "Dev".
sudo -E env CONFIG_PATH="$TMP/config.toml" PORT="$APP_PORT" DEV_ONLY_NO_AUTH=1 \
  DEV_USER_NAME="${DEV_USER_NAME:-Alex Müller}" \
  DEV_USER_EMAIL="${DEV_USER_EMAIL:-alex@plan.ai}" \
  DEV_USER_ADMIN=0 \
  "$WEB/server" >"$TMP/server.log" 2>&1 &
srv_pid=$!
for _ in $(seq 1 60); do
  curl -sf -o /dev/null "http://localhost:$APP_PORT/" && break || sleep 1
done

echo "==> seeding admin token + plan.ai interface"
hash=$(printf admintoken | sha256sum | cut -d' ' -f1)
psql -h "$SOCK" -p "$PG_PORT" -U postgres wgshots -c \
  "INSERT INTO tokens (token_hash, kind) VALUES ('$hash', 'admin')" >/dev/null
AUTH=(-H "Authorization: Bearer admintoken" -H "content-type: application/json")
gid=$(curl -sf "${AUTH[@]}" -d '{"name":"everyone","patterns":["*"]}' \
  "http://localhost:$APP_PORT/api/v1/groups" | jq -r .id)
curl -sf "${AUTH[@]}" -o /dev/null -d "$(jq -cn --arg gid "$gid" '{
  name: "'"$IFACE"'",
  display_name: "plan.ai",
  listen_port: 51820,
  address: "10.42.0.1/24, fd00:42::1/64",
  endpoint: "vpn.plan.ai:51820",
  dns: "10.42.0.1, fd00:42::1",
  allowed_ips: "10.42.0.0/24, fd00:42::/64",
  group_ids: [$gid],
  backend: {kind: "self-managed"}
}')" "http://localhost:$APP_PORT/api/v1/interfaces"

echo "==> capturing screenshots (en + de)"
APP_URL="http://localhost:$APP_PORT" node scripts/regen-tutorial-screenshots.mjs

echo "==> done"
