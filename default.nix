with import <nixpkgs> {};

pkgs.mkShell {
    buildInputs = [ stdenv.cc pkgconfig sqlite.dev xdg_utils ];
}
