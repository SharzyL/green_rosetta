{ stdenv
, lib
, rustPlatform
, runCommand

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
      fetcherVersion = 4;
      hash = "sha256-G7pScDTDL4zEWn7gK3vY/srZZvWH31NcWiQ0F7nKlfM=";
    };

    postInstall = ''
      pnpm build
      mkdir -p $out
      cp dist -rT $out
    '';
  });

  bin = rustPlatform.buildRustPackage {
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

    cargoHash = "sha256-BVgEZdnEOWf47DRnewIUdkvobet4VLmuLT8zY/I46yA=";

    # Build both workspace members
    cargoBuildFlags = [ "--workspace" ];

    doCheck = true;
  };
in
runCommand bin.name { } ''
  mkdir -p $out/share/${pname}/www $out/bin
  cp ${frontend} -rT $out/share/${pname}/www/
  cp ${bin}/bin -rT $out/bin
'' // {
  passthru = { inherit bin frontend; };
  inherit pname version;
}
