{ pkgs, ... }:

let
  make = import ./scripts/make-cli.nix { inherit pkgs; };
  tracy = pkgs.callPackage ./tracy.nix { };
in {
  packages = [
    (make.mkCli (import ./make.nix))
    pkgs.bacon
    pkgs.bun
    pkgs.wtype
    pkgs.gh
    pkgs.pkg-config
    pkgs.fontconfig
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.libGL
    tracy
  ];

  env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.libGL
    pkgs.xorg.libX11
    pkgs.xorg.libXcursor
    pkgs.xorg.libXrandr
    pkgs.xorg.libXi
  ];

  languages.rust = {
    enable = true;
    channel = "nightly";
    components = [ "rustc" "cargo" "rust-src" "rustfmt" "rust-analyzer" "clippy" ];
    targets = [ "wasm32-unknown-unknown" "x86_64-unknown-linux-gnu" ];
  };

  enterShell = ''
    [ -f .localrc ] && source .localrc
    dev
  '';

  dotenv.enable = true;
}
