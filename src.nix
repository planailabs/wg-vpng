# The Rust packages' source: only what their builds read, so a commit to CI,
# docs, tests or the Vue client does not rebuild them. Both need the whole
# workspace for cargo to resolve it.
lib:
lib.fileset.toSource {
  root = ./.;
  fileset = lib.fileset.unions [
    ./Cargo.toml
    ./Cargo.lock
    ./common
    ./design
    ./mikrotik-api
    ./wg-backend
    ./node
    ./server
  ];
}
