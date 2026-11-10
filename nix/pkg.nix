{ lib
, rustPlatform
}:

# The generator CLI. The player and API run on Cloudflare Workers (see wrangler.toml).
rustPlatform.buildRustPackage {
  pname = "green-rosetta";
  version = "0.1.0";

  src = with lib.fileset; toSource {
    root = ./..;
    fileset = unions [
      ../Cargo.toml
      ../Cargo.lock
      ../generator
    ];
  };

  # Vendored straight from the lockfile, so there is no hash to bump when dependencies change.
  cargoLock.lockFile = ../Cargo.lock;

  doCheck = true;
}
