{ ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      packages = {
        fs-plugin = pkgs.rustPlatform.buildRustPackage {
          pname = "totaldroid-wfx";
          version = (fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = ../..;
          cargoLock.lockFile = ../../Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "fs_plugin"
          ];
          cargoTestFlags = [
            "-p"
            "fs_plugin"
          ];

          installPhase = ''
            runHook preInstall
            mkdir -p "$out/lib"
            install -m 755 \
              "target/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/release/libfs_plugin.dylib" \
              "$out/lib/TotalDroid.WFX"
            runHook postInstall
          '';

          meta = {
            description = "TotalDroid Double Commander WFX plugin";
            platforms = pkgs.lib.platforms.darwin;
          };
        };
      };
    };
}
