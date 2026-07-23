{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.wg-vpng-node;
  settingsFormat = pkgs.formats.toml { };
  # api_key is assembled at runtime from apiKeyFile (kept out of the store), so
  # it must not be in the store-rendered base config.
  configBase = settingsFormat.generate "wg-vpng-node.base.toml" cfg.settings;
  apiPort = lib.toInt (lib.last (lib.splitString ":" cfg.settings.bind));
in
{
  options.services.wg-vpng-node = {
    enable = lib.mkEnableOption "wg-vpng-node — a dumb WireGuard switch";

    package = lib.mkPackageOption pkgs "wg-vpng-node" { };

    settings = lib.mkOption {
      type = settingsFormat.type;
      default = { };
      description = ''
        Node config rendered to config.toml (see node/config.example.toml).
        `bind` and `backend` are required; put `api_key` here for testing, or
        leave it out and use `apiKeyFile` to keep it out of the Nix store.
      '';
      example = lib.literalExpression ''
        {
          bind = "0.0.0.0:8787";
          backend = "self-managed";
        }
      '';
    };

    apiKeyFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        File containing the bearer API key (a single line). Appended to the
        runtime config so it never enters the Nix store. Mutually exclusive with
        `settings.api_key`.
      '';
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the control-API TCP port (WireGuard UDP ports are per-interface).";
    };
  };

  config = lib.mkIf cfg.enable {
    services.wg-vpng-node.settings.bind = lib.mkDefault "0.0.0.0:8787";
    services.wg-vpng-node.settings.backend = lib.mkDefault "self-managed";

    assertions = [{
      assertion = (cfg.apiKeyFile != null) != (cfg.settings ? api_key);
      message = "wg-vpng-node: set exactly one of services.wg-vpng-node.apiKeyFile or settings.api_key.";
    }];

    systemd.services.wg-vpng-node = {
      description = "wg-vpng-node — dumb WireGuard switch";
      after = [ "network.target" "systemd-networkd.service" ];
      wants = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      # wg/ip (self-managed) + networkctl (systemd-networkd) for the local
      # backends.
      path = [ pkgs.wireguard-tools pkgs.iproute2 pkgs.systemd ];

      preStart = ''
        umask 077
        # `cat >` (not cp) so the runtime file is a fresh 0600 writable file —
        # cp would inherit the store base config's read-only mode, breaking the
        # api_key append below.
        cat ${configBase} > "$RUNTIME_DIRECTORY/config.toml"
        ${lib.optionalString (cfg.apiKeyFile != null) ''
          printf 'api_key = "%s"\n' "$(cat ${cfg.apiKeyFile})" >> "$RUNTIME_DIRECTORY/config.toml"
        ''}
      '';

      serviceConfig = {
        ExecStart = "${lib.getExe cfg.package} /run/wg-vpng-node/config.toml";
        Restart = "on-failure";
        RestartSec = 5;

        RuntimeDirectory = "wg-vpng-node";
        User = "wg-vpng-node";
        Group = "wg-vpng-node";

        # Creating a kernel WireGuard interface needs NET_ADMIN.
        AmbientCapabilities = [ "CAP_NET_ADMIN" ];
        CapabilityBoundingSet = [ "CAP_NET_ADMIN" ];

        NoNewPrivileges = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        # The systemd-networkd + NetworkManager backends write config files.
        ReadWritePaths = [ "-/etc/systemd/network" "-/etc/NetworkManager/system-connections" ];
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" "AF_NETLINK" ];
        SystemCallArchitectures = "native";
      };
    };

    users.users.wg-vpng-node = {
      isSystemUser = true;
      group = "wg-vpng-node";
      # The systemd-networkd backend writes 0640 root:systemd-network .netdev
      # files; membership lets networkd read them.
      extraGroups = [ "systemd-network" ];
    };
    users.groups.wg-vpng-node = { };

    networking.firewall = lib.mkIf cfg.openFirewall {
      allowedTCPPorts = [ apiPort ];
    };
  };
}
