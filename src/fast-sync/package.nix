{ ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      fast-sync = pkgs.rustPlatform.buildRustPackage {
        pname = "fast-sync";
        version = (fromTOML (builtins.readFile ./Cargo.toml)).package.version;
        src = ../..;
        cargoLock.lockFile = ../../Cargo.lock;
        cargoBuildFlags = [
          "-p"
          "fast-sync"
        ];
        cargoTestFlags = [
          "-p"
          "fast-sync"
        ];

        meta = {
          description = "A CLI for syncing files from WiFi File Transfer";
          mainProgram = "fast-sync";
          platforms = pkgs.lib.platforms.unix;
        };
      };
    in
    {
      packages = {
        default = fast-sync;
        inherit fast-sync;
      };
    };
}
