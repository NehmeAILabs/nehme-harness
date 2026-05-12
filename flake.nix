{
  description = "Minimalistic coding agent written in Rust, optimized for memory footprint and performance";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = manifest.name;
          version = manifest.version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
          ];

          buildFeatures = [ "acp" "memory" "multithread" ];

          meta = with pkgs.lib; {
            description = manifest.description;
            license = licenses.gpl3Only;
            homepage = manifest.homepage;
            mainProgram = "nh";
            platforms = platforms.linux ++ platforms.darwin;
          };
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/nh";
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          buildInputs = with pkgs; [
            rust-analyzer
            rustfmt
            clippy
          ];
        };
      }
    );
}
