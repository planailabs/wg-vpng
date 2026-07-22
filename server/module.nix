{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.wg-vpng-server;
  settingsFormat = pkgs.formats.toml { };
  configFile = settingsFormat.generate "config.toml" cfg.settings;
in
{
  options.services.wg-vpng-server = {
    enable = lib.mkEnableOption "wg-vpng WireGuard VPN generator";

    package = lib.mkPackageOption pkgs "wg-vpng-server" { };

    settings = lib.mkOption {
      type = settingsFormat.type;
      default = { };
      description = ''
        Configuration for wg-vpng-server, rendered to config.toml. See
        config.example.toml for the full shape.
      '';
      example = lib.literalExpression ''
        {
          web.port = 8080;
          # Interfaces (and their backends) are created in the admin UI, not here.
          secrets.encryption_key = "generate-with-openssl-rand-base64-32";
          auth = {
            cookie_secret = "generate-with-openssl-rand-hex-32";
            external_url = "https://vpn.example.com";
            admin_emails = [ "admin@example.com" ];
            providers = [{
              slug = "google";
              name = "Google";
              issuer = "https://accounts.google.com";
              client_id = "xxx.apps.googleusercontent.com";
              client_secret = "GOCSPX-xxx";
              allowed_domains = [ "example.com" ];
            }];
          };
        }
      '';
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        File with environment variables (secrets) as KEY=VALUE lines, kept out
        of the Nix store.
      '';
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the web port in the firewall (WireGuard ports are per-interface).";
    };
  };

  config = lib.mkIf cfg.enable {
    services.wg-vpng-server.settings.database.url = lib.mkDefault "postgres:///wg-vpng?host=/run/postgresql";

    systemd.services.wg-vpng-server = {
      description = "wg-vpng WireGuard VPN generator";
      after = [ "network.target" "postgresql.service" ];
      wants = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      environment.CONFIG_PATH = configFile;
      # wg / ip for the self-managed + NetworkManager backends.
      path = [ pkgs.wireguard-tools pkgs.iproute2 ];

      serviceConfig = {
        ExecStart = lib.getExe cfg.package;
        Restart = "on-failure";
        RestartSec = 5;

        # The self-managed backend creates a kernel WireGuard interface, which
        # needs NET_ADMIN and a non-dynamic user. Other backends work fine here
        # too (they just don't use the capability).
        User = "wg-vpng";
        Group = "wg-vpng";
        StateDirectory = "wg-vpng";
        AmbientCapabilities = [ "CAP_NET_ADMIN" ];
        CapabilityBoundingSet = [ "CAP_NET_ADMIN" ];

        # Hardening (relaxed where the backend needs kernel/network access).
        NoNewPrivileges = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        PrivateTmp = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        RestrictRealtime = true;
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" "AF_NETLINK" ];
        SystemCallArchitectures = "native";
      } // lib.optionalAttrs (cfg.environmentFile != null) {
        EnvironmentFile = cfg.environmentFile;
      };
    };

    users.users.wg-vpng = {
      isSystemUser = true;
      group = "wg-vpng";
    };
    users.groups.wg-vpng = { };

    services.postgresql = {
      enable = true;
      ensureDatabases = [ "wg-vpng" ];
      ensureUsers = [{
        name = "wg-vpng";
        ensureDBOwnership = true;
      }];
    };

    # Interfaces (and their listen ports) are created at runtime in the admin
    # UI, so only the web port is opened here; open per-interface WireGuard UDP
    # ports yourself (or set a range) as you create them.
    networking.firewall = lib.mkIf cfg.openFirewall {
      allowedTCPPorts = [ (cfg.settings.web.port or 8080) ];
    };
  };
}
