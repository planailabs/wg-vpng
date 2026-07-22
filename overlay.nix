{ gitSha ? "unknown" }:

final: prev:
let
  # Patched dioxus-cli with --skip-platform-features and --embed (public
  # assets baked into the server binary via rust-embed). Same patch mac-mgmt
  # and web-agency use.
  dioxus-cli-patched = prev.dioxus-cli.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [
      ./patches/dioxus-cli-all.patch
    ];
  });
in
{
  inherit dioxus-cli-patched;

  wg-vpng-server = prev.callPackage ./server/package.nix { inherit gitSha; };
}
