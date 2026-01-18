{
  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-parts.url = "flake-parts";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { flake-parts, ... }@inputs:
    let
      name = "green-rosetta";

      makePkg = import ./nix/pkg.nix;

      shellOverride = pkgs: oldAttrs: {
        name = "${name}-dev-shell";
        version = null;
        src = null;
        nativeBuildInputs = (oldAttrs.nativeBuildInputs or [ ]) ++ (with pkgs; [
          clippy
          pnpm
          nodejs
          ffmpeg
          sqlite
          pkg-config
        ]);
        shellHook = ''
          unset RUST_LOG
        '';
      };

      overlay = final: _: { ${name} = final.callPackage makePkg { }; };

    in
    # flake-parts boilerplate
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.treefmt-nix.flakeModule
      ];

      flake.overlays.default = overlay;
      flake.nixosModules.default = import ./nix/nixos-module.nix;

      systems = inputs.nixpkgs.lib.systems.flakeExposed;

      perSystem = { system, config, pkgs, ... }: {
        packages.default = config.legacyPackages.${name};
        packages.${name} = config.packages.default;
        legacyPackages = pkgs;

        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [ overlay ];
        };

        devShells.default = config.packages.default.overrideAttrs (shellOverride pkgs);

        treefmt = {
          programs.rustfmt.enable = true;
          programs.nixpkgs-fmt.enable = true;
        };
      };
    };
}
