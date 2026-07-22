# Generic wg-vpng backend integration test.
#
# One shared flow (seed admin token → create device → dual-stack config →
# regenerate → ban/unban → admin subnet device → delete) exercised against a
# given backend. Backend-specific bits are injected via `verifyPy`, which must
# define Python helpers:
#   setup(m)                — wait for backend prerequisites
#   present(m, pub)         — assert the interface carries peer `pub`
#   absent(m, pub)          — assert it does not
#   check_iface(m)          — assert the interface exists / is dual-stack
#   check_subnet(m, cidr, pub) — assert a routed subnet landed for `pub`
#
# `node` is an extra NixOS module with backend prerequisites (kernel module,
# NetworkManager, the fake MikroTik server, …).
{
  pkgs,
  name,
  wireguardBackend,
  node ? { ... }: { },
  verifyPy,
}:

pkgs.testers.runNixOSTest {
  name = "wg-vpng-${name}";

  nodes.machine =
    { lib, ... }:
    {
      imports = [ ../server/module.nix node ];

      environment.systemPackages = with pkgs; [ curl jq wireguard-tools ];

      services.wg-vpng-server = {
        enable = true;
        # pkgs here already carries the flake overlay.
        package = pkgs.wg-vpng-server;
        settings = {
          web.port = 8080;
          wireguard = {
            backend = wireguardBackend;
            interface_name = "wg0";
            listen_port = 51820;
            # Dual-stack (IPv6 non-optional).
            address = "10.8.0.1/24, fd00:8::1/64";
            endpoint = "vpn.example.com:51820";
            dns = "10.8.0.1, fd00:8::1";
            allowed_ips = "10.8.0.0/24, fd00:8::/64";
          };
          # No [auth]: the web UI is open and the API uses bearer tokens, so
          # the test needs no external OIDC provider.
        };
      };
    };

  testScript = ''
    ${verifyPy}

    machine.wait_for_unit("postgresql.service")
    setup(machine)
    machine.wait_for_unit("wg-vpng-server.service")
    machine.wait_for_open_port(8080)

    # Seed an admin API token: token_hash = sha256("admintoken").
    hash = machine.succeed("printf admintoken | sha256sum | cut -d' ' -f1").strip()
    machine.succeed(
        f"sudo -u wg-vpng psql wg-vpng -c \"INSERT INTO tokens (token_hash, kind) VALUES ('{hash}', 'admin')\""
    )
    auth = "-H 'Authorization: Bearer admintoken'"

    # Create a device for a user.
    peer = machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        "-d '{\"user_email\":\"alice@example.com\",\"name\":\"laptop\"}' "
        "http://localhost:8080/api/v1/peers"
    )
    print("created:", peer)
    pid = machine.succeed(f"echo '{peer}' | jq -r .id").strip()
    pub = machine.succeed(f"echo '{peer}' | jq -r .public_key").strip()
    uid = machine.succeed(f"echo '{peer}' | jq -r .user_id").strip()

    # Dual-stack address (v4 + v6).
    addr = machine.succeed(f"echo '{peer}' | jq -r .address").strip()
    assert "10.8.0." in addr and "fd00:8::" in addr, addr

    present(machine, pub)
    check_iface(machine)

    # Rendered client config is a valid-looking, dual-stack WireGuard file.
    cfg = machine.succeed(
        f"curl -sf -X POST {auth} http://localhost:8080/api/v1/peers/{pid}/config | jq -r .config"
    )
    assert "[Interface]" in cfg, cfg
    assert "PrivateKey = " in cfg, cfg
    assert "Endpoint = vpn.example.com:51820" in cfg, cfg
    assert "fd00:8::" in cfg, cfg

    # Regenerate: the public key changes and the backend follows.
    regen = machine.succeed(
        f"curl -sf -X POST {auth} http://localhost:8080/api/v1/peers/{pid}/regenerate"
    )
    newpub = machine.succeed(f"echo '{regen}' | jq -r .public_key").strip()
    assert newpub != pub, f"regenerate did not change key: {newpub}"
    present(machine, newpub)
    absent(machine, pub)

    # Ban the owner: their device is dropped; unban restores it.
    machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        f"-d '{{\"banned\":true}}' http://localhost:8080/api/v1/users/{uid}/ban"
    )
    absent(machine, newpub)
    machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        f"-d '{{\"banned\":false}}' http://localhost:8080/api/v1/users/{uid}/ban"
    )
    present(machine, newpub)

    # Admin assigns a whole IPv6 /64 subnet to a device (a routed network).
    subnet_peer = machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        "-d '{\"user_email\":\"site@example.com\",\"name\":\"branch\",\"address\":\"fd00:beef::/64\"}' "
        "http://localhost:8080/api/v1/peers"
    )
    subpub = machine.succeed(f"echo '{subnet_peer}' | jq -r .public_key").strip()
    present(machine, subpub)
    check_subnet(machine, "fd00:beef::/64", subpub)

    # Delete: the device is removed from the backend.
    machine.succeed(f"curl -sf -X DELETE {auth} http://localhost:8080/api/v1/peers/{pid}")
    absent(machine, newpub)

    # The web UI serves its shell. (Write to a file first: piping curl into
    # `grep -q` trips the test's pipefail — grep closes the pipe early and curl
    # exits 23/EPIPE.)
    machine.succeed("curl -sf http://localhost:8080/ -o /tmp/index.html")
    machine.succeed("grep -qi 'wg-vpng' /tmp/index.html")
  '';
}
