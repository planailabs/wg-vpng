{
  lib,
  rustPlatform,
  makeWrapper,
  wireguard-tools,
  iproute2,
  systemd,
}:

rustPlatform.buildRustPackage {
  pname = "wg-vpng-node";
  version = "0.1.0";
  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;
  cargoLock.outputHashes = import ../extra-hashes.nix;

  # Build just the node binary from the workspace; the default build would pull
  # in the dx/wasm server, which this package doesn't need.
  cargoBuildFlags = [ "-p" "wg-vpng-node" ];
  doCheck = false;

  nativeBuildInputs = [ makeWrapper ];

  postInstall = ''
    # wg/ip (self-managed) + networkctl (systemd-networkd) on PATH for the
    # local backends the node applies with.
    wrapProgram $out/bin/wg-vpng-node \
      --prefix PATH : ${lib.makeBinPath [ wireguard-tools iproute2 systemd ]}
  '';

  meta = {
    description = "wg-vpng-node — a dumb WireGuard switch driven by the wg-vpng server";
    license = lib.licenses.asl20;
    mainProgram = "wg-vpng-node";
  };
}
