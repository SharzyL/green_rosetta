{ stdenv
, lib
, rustPlatform

, pnpm
, pnpmConfigHook
, fetchPnpmDeps
, nodejs
}:
let
  # Build the frontend
  pname = "green-rosetta";
  version = "0.1.0";
  frontend = stdenv.mkDerivation (finalAttrs: {
    pname = "${pname}-frontend";
    inherit version;

    src = ../frontend;

    nativeBuildInputs = [
      nodejs
      pnpm
      pnpmConfigHook
    ];

    pnpmDeps = fetchPnpmDeps {
      inherit (finalAttrs) pname version src;
      fetcherVersion = 3;
      hash = "sha256-gVlA+yuWKw6PK+u3lGt8jIzzilYRbg5Q6sREhNL0Y2c=";
    };

    postInstall = ''
      pnpm build
      mkdir -p $out
      cp dist -rT $out
    '';
  });

in
# Build the Rust workspace
rustPlatform.buildRustPackage {
  inherit pname version;

  src = with lib.fileset; toSource {
    root = ./..;
    fileset = unions [
      ../Cargo.toml
      ../Cargo.lock
      ../backend
      ../generator
    ];
  };

  passthru = { inherit frontend; };

  cargoHash = "sha256-MjEJf6ljQR6Rs4YCm1vObmXFb8yZl0zbtuCZEih49RY=";

  # Build both workspace members
  cargoBuildFlags = [ "--workspace" ];

  postInstall = ''
    mkdir -p $out/share/${pname}/www
    cp ${frontend} -rT $out/share/${pname}/www/
  '';

  doCheck = true;
}
