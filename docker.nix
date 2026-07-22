# OCI image for wg-vpng-server, built with nix's dockerTools (no Dockerfile /
# daemon). The server binary is a makeWrapper wrapper that already puts
# wg/ip on PATH; cacert is added for outbound TLS (OIDC, MikroTik REST).
{
  pkgs,
  wg-vpng-server,
  tag ? "latest",
}:

let
  cacert = pkgs.cacert;
in
pkgs.dockerTools.buildLayeredImage {
  name = "wg-vpng";
  inherit tag;
  contents = [
    wg-vpng-server
    cacert
    pkgs.wireguard-tools
    pkgs.iproute2
  ];
  config = {
    Entrypoint = [ "${wg-vpng-server}/bin/wg-vpng-server" ];
    Env = [
      "SSL_CERT_FILE=${cacert}/etc/ssl/certs/ca-bundle.crt"
      "CONFIG_PATH=/config/config.toml"
    ];
    ExposedPorts = {
      "8080/tcp" = { }; # web UI + API
      "51820/udp" = { }; # WireGuard (self-managed backend)
    };
    Volumes = { "/config" = { }; };
  };
}
