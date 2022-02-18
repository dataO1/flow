{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    cargo
    # wasm-pack
    rustc
    rustfmt
    rustPackages.clippy
    # cargo-web
    alsa-lib
    pkg-config
    libpulseaudio
    cmake
  ];
}
