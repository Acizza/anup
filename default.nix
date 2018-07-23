# This script is used for building the program on NixOS with the proper dependencies.
# To use, simply run nix-shell in this directory and run "cargo build" as you normally would.

with import <nixpkgs> {};

stdenv.mkDerivation {
    name = "anup";
    buildInputs = [ buildPackages.stdenv.cc pkgconfig openssl.dev xdg_utils ];
}
