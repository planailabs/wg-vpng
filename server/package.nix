{
  lib,
  rustPlatform,
  fetchurl,
  pkg-config,
  openssl,
  dioxus-cli-patched,
  nodejs,
  wasm-bindgen-cli_0_2_121,
  binaryen,
  tailwindcss_3,
  lld,
  makeWrapper,
  wireguard-tools,
  iproute2,
  gitSha ? "unknown",
}:

let
  # utoipa-swagger-ui's build.rs otherwise downloads Swagger UI with curl.
  swagger-ui = fetchurl {
    url = "https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip";
    hash = "sha256-SBJE0IEgl7Efuu73n3HZQrFxYX+cn5UU5jrL4T5xzNw=";
  };
in

rustPlatform.buildRustPackage {
  pname = "wg-vpng-server";
  version = "0.1.0";
  src = ../.;
  cargoLock.lockFile = ../Cargo.lock;
  cargoLock.outputHashes = import ../extra-hashes.nix;

  cargoBuildFlags = [ "-p" "wg-vpng-server" ];

  nativeBuildInputs = [
    pkg-config
    dioxus-cli-patched
    nodejs
    wasm-bindgen-cli_0_2_121
    binaryen
    tailwindcss_3
    lld
    makeWrapper
  ];

  buildInputs = [ openssl ];

  SWAGGER_UI_DOWNLOAD_URL = "file://${swagger-ui}";
  env.GIT_SHA = gitSha;

  doCheck = false;

  # Build with dx so WASM + assets are bundled and embedded in the server.
  buildPhase = ''
    runHook preBuild

    pushd server
    npm run tailwind:build
    popd

    dx build --release --fullstack --package wg-vpng-server

    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall

    mkdir -p $out/bin $out/share
    cp -r target/dx/wg-vpng-server/release/web $out/share/wg-vpng-server

    # wg/ip on PATH for the self-managed + NetworkManager backends.
    bin=$out/share/wg-vpng-server/wg-vpng-server
    [ -e "$bin" ] || bin=$out/share/wg-vpng-server/server
    makeWrapper "$bin" $out/bin/wg-vpng-server \
      --prefix PATH : ${lib.makeBinPath [ wireguard-tools iproute2 ]}

    runHook postInstall
  '';

  meta = {
    description = "WireGuard VPN generator — OIDC, per-user configs, pluggable backends";
    license = lib.licenses.asl20;
    mainProgram = "wg-vpng-server";
  };
}
