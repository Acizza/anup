{}:

let
  rustOverlay = import (builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz");

  pkgs = import <nixpkgs> {
    overlays = [ rustOverlay ];
  };

  rust = pkgs.rust-bin.stable.latest.rust.override {
    extensions = [ "rust-src" ];
  };
in pkgs.mkShell {
  buildInputs = with pkgs; [
    rust
    sqlite.dev
    xdg_utils
  ];
}
