{
  description = "A lightweight Linux tray application which shows Claude usage and limits";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: {
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.claude-usage-tray;
        claude-usage-tray = pkgs.rustPlatform.buildRustPackage {
          pname = "claude-usage-tray";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;

          postInstall = ''
            install -Dm644 dist/claude-usage-tray.desktop -t $out/share/applications
            install -Dm644 assets/default-icon.png $out/share/icons/hicolor/128x128/apps/claude-usage-tray.png
            install -Dm644 dist/claude-usage-tray.service -t $out/share/systemd/user
            substituteInPlace $out/share/systemd/user/claude-usage-tray.service \
              --replace-fail '%h/.cargo/bin/claude-usage-tray' "$out/bin/claude-usage-tray"
          '';

          meta = {
            description = "A lightweight Linux tray application which shows Claude usage and limits";
            homepage = "https://github.com/duckysmacky/claude-usage-tray";
            license = nixpkgs.lib.licenses.mit;
            platforms = nixpkgs.lib.platforms.linux;
            mainProgram = "claude-usage-tray";
          };
        };
      });

      nixosModules.default =
        { config, lib, pkgs, ... }:
        let
          cfg = config.services.claude-usage-tray;
        in
        {
          options.services.claude-usage-tray = {
            enable = lib.mkEnableOption "Claude usage tray";
            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              description = "The claude-usage-tray package to use.";
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ cfg.package ];
            systemd.user.services.claude-usage-tray = {
              description = "Claude usage tray";
              after = [ "graphical-session.target" ];
              partOf = [ "graphical-session.target" ];
              wantedBy = [ "graphical-session.target" ];
              serviceConfig = {
                ExecStart = lib.getExe cfg.package;
                Restart = "on-failure";
                RestartSec = 5;
              };
            };
          };
        };

      homeManagerModules.default =
        { config, lib, pkgs, ... }:
        let
          cfg = config.services.claude-usage-tray;
        in
        {
          options.services.claude-usage-tray = {
            enable = lib.mkEnableOption "Claude usage tray";
            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              description = "The claude-usage-tray package to use.";
            };
          };

          config = lib.mkIf cfg.enable {
            home.packages = [ cfg.package ];
            systemd.user.services.claude-usage-tray = {
              Unit = {
                Description = "Claude usage tray";
                After = [ "graphical-session.target" ];
                PartOf = [ "graphical-session.target" ];
              };
              Service = {
                ExecStart = lib.getExe cfg.package;
                Restart = "on-failure";
                RestartSec = 5;
              };
              Install.WantedBy = [ "graphical-session.target" ];
            };
          };
        };
    };
}
