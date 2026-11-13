# devenv.nix
{ pkgs, ... }:

let
  tracy = pkgs.callPackage ./tracy.nix { };
  # `dev` launcher on PATH: delegates to the nushell toolbox. Root resolves
  # via DEVENV_ROOT inside the shell, else the enclosing git checkout, so the
  # command works from any subdirectory.
  devLauncher = pkgs.writeShellScriptBin "dev" ''
    root="''${DEVENV_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null)}"
    if [ -z "$root" ] || [ ! -f "$root/scripts/toolbox.nu" ]; then
      echo "dev: not in the spleen checkout (no DEVENV_ROOT, no git root)" >&2
      exit 2
    fi
    exec ${pkgs.nushell}/bin/nu "$root/scripts/toolbox.nu" "$@"
  '';
in {
  packages = [
    devLauncher
    pkgs.bacon
    pkgs.bun
    pkgs.wtype
    pkgs.gh
    pkgs.pkg-config
    pkgs.fontconfig
    pkgs.wayland
    pkgs.pnpm
    pkgs.libxkbcommon
    pkgs.libGL
    pkgs.vulkan-loader
    # riode-glass Hyprland plugin (C++): compiler, build tool, compositor
    # headers + everything hyprland.pc Requires so pkg-config resolves.
    pkgs.gcc
    pkgs.gnumake
    pkgs.hyprland
    pkgs.aquamarine
    pkgs.hyprcursor
    pkgs.hyprgraphics
    pkgs.hyprlang
    pkgs.hyprutils
    pkgs.pixman
    pkgs.libdrm
    pkgs.cairo
    pkgs.libinput
    pkgs.glslang
    pkgs.xorg.libxcb
    pkgs.xorg.xcbutil
    pkgs.xorg.xcbutilerrors
    pkgs.xorg.xcbutilwm
   # tracy
  ];

  env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.wayland
    pkgs.libxkbcommon
    pkgs.libGL
    pkgs.vulkan-loader
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

  # in devenv.nix
  env.VK_DRIVER_FILES = "${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json:${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json";

  enterShell = ''
    [ -f .localrc ] && source .localrc

    # Auto-register `dev` with Carapace
    spec_dir="''${XDG_CONFIG_HOME:-$HOME/.config}/carapace/specs"
    mkdir -p "$spec_dir"
    ln -sf "''${DEVENV_ROOT:-$PWD}/scripts/dev.yaml" "$spec_dir/dev.yaml"

    dev
  '';

  dotenv.enable = true;
}
