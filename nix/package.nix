{ ... }:
{
  perSystem =
    { pkgs, ... }:
    let
      fast-sync = pkgs.rustPlatform.buildRustPackage {
        pname = "fast-sync";
        version = "0.0.1";
        src = ../.;
        cargoLock.lockFile = ../Cargo.lock;

        meta = {
          description = "A client for the WiFi File Transfer HTTP protocol";
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

      devShells.default = pkgs.mkShell {
        packages = with pkgs; [
          cargo
          clippy
          rust-analyzer
          rustc
          rustfmt
        ];

        RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      };
    };
}
