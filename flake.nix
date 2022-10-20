{
  description = "A basic flake with a shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-22.05";
  inputs.flake-utils.url = "github:numtide/flake-utils";

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShell = with pkgs; pkgs.mkShell {

          nativeBuildInputs = [
            cargo
            # wasm-pack
            # cargo-web
            rustc
            rustfmt
            rustPackages.clippy
            pkg-config
          ];
          buildInputs = [
            alsa-lib
            libpulseaudio
          ];
        };
      });
}
