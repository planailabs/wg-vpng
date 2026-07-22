# Generic wg-vpng backend integration test.
#
# One shared flow (seed admin token → create an interface with the given
# backend → create device → dual-stack config → regenerate → ban/unban → admin
# subnet device → delete) exercised against a backend. Interfaces are
# admin-managed, so the test creates one via the API (from ./interface.json,
# whose `backend` is the `backend` arg). Backend-specific assertions come from
# `verifyPy`, which must define Python helpers:
#   setup(m), present(m, pub), absent(m, pub), check_iface(m),
#   check_subnet(m, cidr, pub)
#
# `node` is an extra NixOS module with backend prerequisites; `settingsExtra`
# is merged into services.wg-vpng-server.settings (e.g. [secrets] for mikrotik).
{
  pkgs,
  name,
  backend,
  node ? { ... }: { },
  settingsExtra ? { },
  verifyPy,
}:

let
  interfaceJson = builtins.toJSON {
    name = "wg0";
    listen_port = 51820;
    address = "10.8.0.1/24, fd00:8::1/64"; # dual-stack (IPv6 non-optional)
    endpoint = "vpn.example.com:51820";
    dns = "10.8.0.1, fd00:8::1";
    allowed_ips = "10.8.0.0/24, fd00:8::/64";
    access_patterns = [ "*" ]; # open interface for the test
    inherit backend;
  };
in
pkgs.testers.runNixOSTest {
  name = "wg-vpng-${name}";

  nodes.machine =
    { lib, ... }:
    {
      imports = [ ../server/module.nix node ];

      environment.systemPackages = with pkgs; [ curl jq wireguard-tools ];
      # Interface-creation payload posted by the test (avoids JSON-in-shell quoting).
      environment.etc."wg-vpng-test/interface.json".text = interfaceJson;

      services.wg-vpng-server = {
        enable = true;
        package = pkgs.wg-vpng-server;
        settings = lib.recursiveUpdate { web.port = 8080; } settingsExtra;
      };
    };

  testScript = ''
    ${verifyPy}

    machine.wait_for_unit("postgresql.service")
    setup(machine)
    machine.wait_for_unit("wg-vpng-server.service")
    machine.wait_for_open_port(8080)

    # Seed an admin API token: token_hash = sha256("admintoken"). Seed as the
    # postgres superuser so it works whichever role owns the tables (the NM
    # backend runs the service as root; others as wg-vpng).
    hash = machine.succeed("printf admintoken | sha256sum | cut -d' ' -f1").strip()
    machine.succeed(
        f"sudo -u postgres psql wg-vpng -c \"INSERT INTO tokens (token_hash, kind) VALUES ('{hash}', 'admin')\""
    )
    auth = "-H 'Authorization: Bearer admintoken'"

    # Create the interface (admin-managed) with this backend.
    code = machine.succeed(
        "curl -s -o /tmp/ifresp -w '%{http_code}' -X POST " + auth + " -H 'content-type: application/json' "
        "-d @/etc/wg-vpng-test/interface.json http://localhost:8080/api/v1/interfaces"
    ).strip()
    if code != "200":
        print("interface create HTTP", code, "body:", machine.succeed("cat /tmp/ifresp"))
        print(machine.succeed("journalctl -u wg-vpng-server --no-pager | tail -30"))
    assert code == "200", f"interface create returned HTTP {code}"

    # Create a device for a user (single interface -> id can be omitted).
    peer = machine.succeed(
        f"curl -sf -X POST {auth} -H 'content-type: application/json' "
        "-d '{\"user_email\":\"alice@example.com\",\"name\":\"laptop\"}' "
        "http://localhost:8080/api/v1/peers"
    )
    print("created:", peer)
    pid = machine.succeed(f"echo '{peer}' | jq -r .id").strip()
    pub = machine.succeed(f"echo '{peer}' | jq -r .public_key").strip()
    uid = machine.succeed(f"echo '{peer}' | jq -r .user_id").strip()

    addr = machine.succeed(f"echo '{peer}' | jq -r .address").strip()
    assert "10.8.0." in addr and "fd00:8::" in addr, addr

    present(machine, pub)
    check_iface(machine)

    cfg = machine.succeed(
        f"curl -sf -X POST {auth} http://localhost:8080/api/v1/peers/{pid}/config | jq -r .config"
    )
    assert "[Interface]" in cfg, cfg
    assert "PrivateKey = " in cfg, cfg
    assert "Endpoint = vpn.example.com:51820" in cfg, cfg
    assert "fd00:8::" in cfg, cfg

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

    # The web UI serves its shell (write to a file first: piping curl into
    # `grep -q` trips pipefail — grep closes the pipe early and curl exits 23).
    machine.succeed("curl -sf http://localhost:8080/ -o /tmp/index.html")
    machine.succeed("grep -qi 'wg-vpng' /tmp/index.html")
  '';
}
