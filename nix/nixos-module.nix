{ config, pkgs, lib, ... }:

let
  cfg = config.services.green-rosetta;
in
{
  options.services.green-rosetta = with lib; {
    enable = mkEnableOption "Green rosetta service";

    package = lib.mkPackageOption pkgs "green-rosetta" { };

    listen = lib.mkOption { type = types.str; };
    configFile = mkOption { type = types.path; };
    envFile = lib.mkOption { type = types.path; };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.green-rosetta = {
      description = "green-rosetta backend";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        EnvironmentFile = cfg.envFile;

        ExecStart = ''
          ${cfg.package}/bin/green-rosetta-backend --config ${cfg.configFile} \
            --listen ${cfg.listen} \
            --web-dir ${cfg.package}/share/${cfg.package.pname}/www
        '';
        User = "green-rosetta";
        StateDirectory = "green-rosetta";
        Restart = "on-failure";
        ReadOnlyPaths = "/";
        ReadWritePaths = "%S/green-rosetta";

        # hardening
        RemoveIPC = true;
        ProtectSystem = "strict";
        PrivateTmp = true;
        NoNewPrivileges = true;
        RestrictSUIDSGID = true;
        ProtectHome = true;
        UMask = "0077";

        ProtectHostname = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        PrivateUsers = true;
        PrivateDevices = true;

        ProtectControlGroups = true;
        LockPersonality = true;
        RestrictRealtime = true;
        ProtectClock = true;
        ProtectKernelLogs = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        RestrictNamespaces = true;

        SystemCallArchitectures = "native";

        DynamicUser = true; # implies RemoveIPC, ProtectSystem, PrivateTmp, NoNewPrivileges, RestrictSUIDSGID
        MemoryDenyWriteExecute = true;

        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];

        SystemCallFilter = [ "@system-service" ];

        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" ];
      };
    };
  };
}


