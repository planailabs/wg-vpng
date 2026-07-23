# Instantiate the generic backend test (./lib.nix) for every WireGuard backend:
# self-managed (kernel wg), NetworkManager (kernel wg via nmcli keyfile), and
# MikroTik (RouterOS REST, against the fake server in ./fake-mikrotik.py).
{ pkgs }:

let
  mk = import ./lib.nix;

  # Verify helpers for backends that produce a real kernel wg0 interface
  # (self-managed + NetworkManager). `setupBody` waits for any prerequisite.
  kernelVerify = setupBody: ''
    def setup(m):
        ${setupBody}

    def present(m, pub):
        m.wait_until_succeeds("wg show wg0 | grep -q '" + pub + "'", timeout=25)

    def absent(m, pub):
        m.wait_until_fails("wg show wg0 | grep -q '" + pub + "'", timeout=25)

    def check_iface(m):
        m.wait_until_succeeds("ip addr show wg0 | grep -q '10.8.0.1/24'", timeout=25)
        m.wait_until_succeeds("ip -6 addr show wg0 | grep -qi 'fd00:8::1/64'", timeout=25)

    def check_subnet(m, cidr, pub):
        m.wait_until_succeeds("wg show wg0 allowed-ips | grep -q '" + cidr + "'", timeout=25)
  '';

  # Verify helpers for the MikroTik backend: state lives in the fake router, so
  # assertions query its REST API rather than a local interface.
  mikrotikVerify = ''
    API = "curl -sf -u admin:testpass http://127.0.0.1:8081/rest/interface/wireguard"

    def setup(m):
        m.wait_for_unit("fake-mikrotik.service")
        m.wait_for_open_port(8081)

    def present(m, pub):
        m.wait_until_succeeds(API + "/peers | grep -q '" + pub + "'", timeout=25)

    def absent(m, pub):
        m.wait_until_fails(API + "/peers | grep -q '" + pub + "'", timeout=25)

    def check_iface(m):
        m.wait_until_succeeds(API + " | grep -q wg0", timeout=25)

    def check_subnet(m, cidr, pub):
        m.wait_until_succeeds(API + "/peers | grep -q '" + cidr + "'", timeout=25)
  '';
in
{
  # ── self-managed: kernel wg via wg + ip ────────────────────────────
  integration-self-managed = mk {
    inherit pkgs;
    name = "self-managed";
    backend = { kind = "self-managed"; };
    node = { ... }: { boot.kernelModules = [ "wireguard" ]; };
    verifyPy = kernelVerify "pass";
  };

  # ── NetworkManager: kernel wg via an nmcli keyfile connection ───────
  # wg-vpng writes NM keyfiles under /etc and runs nmcli, so it runs as root
  # here (with a matching postgres superuser role, since peer auth maps the OS
  # user to a pg role). NM is told to leave the test's ethernet alone.
  integration-networkmanager = mk {
    inherit pkgs;
    name = "networkmanager";
    backend = { kind = "network-manager"; };
    node =
      { pkgs, lib, ... }:
      {
        boot.kernelModules = [ "wireguard" ];
        networking.networkmanager.enable = true;
        networking.networkmanager.unmanaged = [ "interface-name:eth*" "interface-name:ens*" ];
        services.postgresql.ensureUsers = [
          { name = "root"; ensureClauses.superuser = true; }
        ];
        systemd.services.wg-vpng-server = {
          after = [ "NetworkManager.service" ];
          wants = [ "NetworkManager.service" ];
          path = [ pkgs.networkmanager pkgs.wireguard-tools pkgs.iproute2 ];
          serviceConfig = {
            User = lib.mkForce "root";
            Group = lib.mkForce "root";
            # Keep ProtectSystem=strict (from the module) but carve out NM's
            # keyfile dir so the backend can write connection profiles there
            # (they must be root-owned, hence User=root). '-' tolerates the dir
            # not existing yet.
            ReadWritePaths = [ "-/etc/NetworkManager/system-connections" ];
          };
        };
      };
    verifyPy = kernelVerify ''m.wait_for_unit("NetworkManager.service")'';
  };

  # ── systemd-networkd: kernel wg via .netdev/.network + networkctl ───
  # Like NetworkManager, wg-vpng writes config under /etc and runs networkctl,
  # so it runs as root (with a matching postgres superuser role).
  integration-systemd-networkd = mk {
    inherit pkgs;
    name = "systemd-networkd";
    backend = { kind = "systemd-networkd"; };
    node =
      { pkgs, lib, ... }:
      {
        boot.kernelModules = [ "wireguard" ];
        systemd.network.enable = true;
        services.postgresql.ensureUsers = [
          { name = "root"; ensureClauses.superuser = true; }
        ];
        systemd.services.wg-vpng-server = {
          after = [ "systemd-networkd.service" ];
          wants = [ "systemd-networkd.service" ];
          path = [ pkgs.systemd pkgs.wireguard-tools pkgs.iproute2 ];
          serviceConfig = {
            User = lib.mkForce "root";
            Group = lib.mkForce "root";
            # Keep ProtectSystem=strict (from the module) but carve out networkd's
            # config dir so the backend can write .netdev/.network there.
            ReadWritePaths = [ "-/etc/systemd/network" ];
          };
        };
      };
    verifyPy = kernelVerify ''m.wait_for_unit("systemd-networkd.service")'';
  };

  # ── node: server drives a local wg-vpng-node over HTTP, which applies ─
  # wg0 with its own self-managed backend. Both run on one machine; the
  # server's Node backend points at 127.0.0.1:8787, so `wg show wg0` reflects
  # what the node created.
  integration-node = mk {
    inherit pkgs;
    name = "node";
    backend = {
      kind = "node";
      node_url = "http://127.0.0.1:8787";
      node_key = "testnodekey";
    };
    # The node key is stored encrypted at rest, so the server needs an app key.
    settingsExtra.secrets.encryption_key =
      "0000000000000000000000000000000000000000000000000000000000000000";
    node =
      { ... }:
      {
        imports = [ ../node/module.nix ];
        boot.kernelModules = [ "wireguard" ];
        services.wg-vpng-node = {
          enable = true;
          package = pkgs.wg-vpng-node;
          settings = {
            bind = "127.0.0.1:8787";
            backend = "self-managed";
            api_key = "testnodekey";
          };
        };
        systemd.services.wg-vpng-server = {
          after = [ "wg-vpng-node.service" ];
          wants = [ "wg-vpng-node.service" ];
        };
      };
    verifyPy = kernelVerify ''
      m.wait_for_unit("wg-vpng-node.service")
      m.wait_for_open_port(8787)
    '';
  };

  # ── MikroTik: RouterOS REST against the fake server ────────────────
  integration-mikrotik = mk {
    inherit pkgs;
    name = "mikrotik";
    backend = {
      kind = "mikrotik";
      mikrotik_url = "http://127.0.0.1:8081";
      mikrotik_username = "admin";
      mikrotik_password = "testpass";
      mikrotik_insecure = true;
    };
    # Storing the RouterOS password needs an app encryption key (hex 32 bytes).
    settingsExtra.secrets.encryption_key =
      "0000000000000000000000000000000000000000000000000000000000000000";
    node =
      { pkgs, ... }:
      {
        systemd.services.fake-mikrotik = {
          description = "Fake MikroTik RouterOS REST API";
          wantedBy = [ "multi-user.target" ];
          before = [ "wg-vpng-server.service" ];
          serviceConfig = {
            ExecStart = "${pkgs.python3}/bin/python3 ${./fake-mikrotik.py} 8081";
            DynamicUser = true;
            Restart = "on-failure";
          };
        };
        systemd.services.wg-vpng-server = {
          after = [ "fake-mikrotik.service" ];
          wants = [ "fake-mikrotik.service" ];
        };
      };
    verifyPy = mikrotikVerify;
  };
}
