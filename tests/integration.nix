# NixOS VM integration test: postgres + wg-vpng with the self-managed backend.
# Drives the plan-ai-api-mcp REST surface with a seeded admin token and asserts
# the kernel WireGuard interface converges (peer create / regenerate / delete).
{ pkgs, ... }:

pkgs.testers.runNixOSTest {
  name = "wg-vpng-integration";

  nodes.machine = { ... }: {
    imports = [ ../server/module.nix ];

    boot.kernelModules = [ "wireguard" ];

    environment.systemPackages = with pkgs; [ curl jq wireguard-tools ];

    services.wg-vpng-server = {
      enable = true;
      # pkgs here already carries the flake overlay (the check is callPackage'd
      # from the overlaid package set), so the built server is available.
      package = pkgs.wg-vpng-server;
      settings = {
        web.port = 8080;
        wireguard = {
          backend = "self-managed";
          interface_name = "wg0";
          listen_port = 51820;
          # Dual-stack (IPv6 non-optional).
          address = "10.8.0.1/24, fd00:8::1/64";
          endpoint = "vpn.example.com:51820";
          dns = "10.8.0.1, fd00:8::1";
          allowed_ips = "10.8.0.0/24, fd00:8::/64";
        };
        # No [auth]: the web UI is open, and the API uses bearer tokens. That
        # keeps the test free of an external OIDC provider.
      };
    };
  };

  testScript = ''
    machine.wait_for_unit("postgresql.service")
    machine.wait_for_unit("wg-vpng-server.service")
    machine.wait_for_open_port(8080)

    # Seed an admin API token: token_hash = sha256("admintoken").
    hash = machine.succeed("printf admintoken | sha256sum | cut -d' ' -f1").strip()
    machine.succeed(
        f"sudo -u wg-vpng psql wg-vpng -c \"INSERT INTO tokens (token_hash, kind) VALUES ('{hash}', 'admin')\""
    )

    auth = "-H 'Authorization: Bearer admintoken'"

    # The interface is seeded on boot and brought up by the self-managed backend.
    machine.wait_until_succeeds("wg show wg0", timeout=30)

    # Dual-stack: the interface carries both the v4 and v6 server addresses.
    machine.wait_until_succeeds("ip addr show wg0 | grep -q '10.8.0.1/24'", timeout=15)
    machine.wait_until_succeeds("ip -6 addr show wg0 | grep -qi 'fd00:8::1/64'", timeout=15)

    # Create a peer for a user.
    peer = machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        "-d '{\"user_email\":\"alice@example.com\",\"name\":\"laptop\"}' "
        "http://localhost:8080/api/v1/peers"
    )
    print("created:", peer)
    pid = machine.succeed(f"echo '{peer}' | jq -r .id").strip()
    pub = machine.succeed(f"echo '{peer}' | jq -r .public_key").strip()
    uid = machine.succeed(f"echo '{peer}' | jq -r .user_id").strip()

    # The peer got a dual-stack address (v4 + v6).
    addr = machine.succeed(f"echo '{peer}' | jq -r .address").strip()
    assert "10.8.0." in addr and "fd00:8::" in addr, addr

    # The kernel interface must now carry that peer.
    machine.wait_until_succeeds(f"wg show wg0 | grep -q '{pub}'", timeout=15)

    # The rendered client config is a valid-looking, dual-stack WireGuard file.
    cfg = machine.succeed(
        f"curl -sf -X POST {auth} http://localhost:8080/api/v1/peers/{pid}/config | jq -r .config"
    )
    assert "[Interface]" in cfg, cfg
    assert "PrivateKey = " in cfg, cfg
    assert "Endpoint = vpn.example.com:51820" in cfg, cfg
    assert "fd00:8::" in cfg, cfg

    # Admin assigns a whole IPv6 /64 subnet to a device (a routed network).
    subnet_peer = machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        "-d '{\"user_email\":\"site@example.com\",\"name\":\"branch\",\"address\":\"fd00:beef::/64\"}' "
        "http://localhost:8080/api/v1/peers"
    )
    subpub = machine.succeed(f"echo '{subnet_peer}' | jq -r .public_key").strip()
    # wg reflects the /64 as an allowed-ip for that peer.
    machine.wait_until_succeeds(f"wg show wg0 allowed-ips | grep -q 'fd00:beef::/64'", timeout=15)
    machine.succeed(f"wg show wg0 | grep -q '{subpub}'")

    # Regenerate: the public key changes and the interface follows.
    regen = machine.succeed(
        f"curl -sf -X POST {auth} http://localhost:8080/api/v1/peers/{pid}/regenerate"
    )
    newpub = machine.succeed(f"echo '{regen}' | jq -r .public_key").strip()
    assert newpub != pub, f"regenerate did not change key: {newpub}"
    machine.wait_until_succeeds(f"wg show wg0 | grep -q '{newpub}'", timeout=15)
    machine.fail(f"wg show wg0 | grep -q '{pub}'")

    # Ban the owner: their device is dropped from the interface; unban restores.
    machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        f"-d '{{\"banned\":true}}' http://localhost:8080/api/v1/users/{uid}/ban"
    )
    machine.wait_until_fails(f"wg show wg0 | grep -q '{newpub}'", timeout=15)
    machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        f"-d '{{\"banned\":false}}' http://localhost:8080/api/v1/users/{uid}/ban"
    )
    machine.wait_until_succeeds(f"wg show wg0 | grep -q '{newpub}'", timeout=15)

    # Delete: the peer is removed from the interface.
    machine.succeed(f"curl -sf -X DELETE {auth} http://localhost:8080/api/v1/peers/{pid}")
    machine.wait_until_fails(f"wg show wg0 | grep -q '{newpub}'", timeout=15)

    # The web UI serves its shell (Dioxus app HTML).
    machine.succeed("curl -sf http://localhost:8080/ | grep -qi 'wg-vpng'")
  '';
}
