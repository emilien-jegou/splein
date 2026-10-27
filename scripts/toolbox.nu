# Spleen dev toolbox (nushell): `nu scripts/toolbox.nu <command> [args]`,
# or just `dev <command> [args]` inside the devenv shell (a `dev` launcher
# on PATH delegates here). Run from anywhere inside the checkout. Bare
# `dev` (or `nu scripts/toolbox.nu`) lists commands.
#
# Tab-completion is driven by Carapace via scripts/dev.yaml.

# Repo root: CWD if it holds examples/, else git toplevel, else CWD.
export def spleen--root [] {
  if ("examples" | path exists) { $env.PWD } else {
    try { git rev-parse --show-toplevel e> /dev/null | str trim } catch { $env.PWD }
  }
}

# Hot-reloadable examples: dirs containing hot.rs (release-only demos never suggest).
export def spleen-complete-examples [] {
  let examples_dir = (spleen--root | path join examples)
  if not ($examples_dir | path exists) { return [] }
  ls $examples_dir
  | where type == dir
  | where { $in.name | path join "hot.rs" | path exists }
  | get name
  | path basename
}

export def spleen-complete-engines [] {
  ["tiny-skia", "wgpu"]
}

# List all commands.
def main [] {
  let cmds = [
    { command: "build", args: "[args...]", about: "Build the project. Extra args pass through to cargo." },
    { command: "test", args: "[args...]", about: "Run cargo tests. Extra args pass through to cargo." },
    { command: "watch", args: "", about: "Run bacon (watch mode)." },
    { command: "example", args: "[name=demo1] [engine]", about: "Run an example in hot-reload mode. Engines: tiny-skia, wgpu." },
    { command: "desktop-glass", args: "", about: "Run the desktop-glass overlay with hot-reloadable view." },
    { command: "load-plugin", args: "", about: "Rebuild riode-glass.so and reload it into Hyprland." },
    { command: "apps", args: "[app=dock]", about: "Run a standalone overlay app with hot-reloadable UI." },
    { command: "relacher", args: "", about: "Generate release commit and git tags." },
    { command: "release-github", args: "", about: "Generate release patch for github and deploy changes." },
  ]
  let plain = ($cmds | each {|c| if $c.args == "" { $c.command } else { $"($c.command) ($c.args)" } })
  let w = ($plain | str length | math max)
  # Colors only on a real terminal: piped output stays plain text.
  let tty = (is-terminal --stdout)
  let hi = if $tty { ansi green_bold } else { "" }
  let dim = if $tty { ansi dark_gray } else { "" }
  let off = if $tty { ansi reset } else { ""}
  print ""
  print $"  ($hi)spleen($off) ($dim)nu scripts/toolbox.nu <command>($off)"
  print ""
  for $c in ($cmds | zip $plain | each {|p| $p.0 | insert left $p.1 }) {
    let pad = ("" | fill -a left -w ($w - ($c.left | str length)) -c " ")
    let head = if $c.args == "" {
      $"  ($hi)($c.command)($off)"
    } else {
      $"  ($hi)($c.command)($off) ($dim)($c.args)($off)"
    }
    print $"($head)($pad)  ($c.about)"
  }
  print ""
}

# Build the project. Extra args pass through to cargo.
def "main build" [...args: string] {
  cd (spleen--root)
  ^cargo build ...$args
}

# Run cargo tests. Extra args pass through to cargo.
def "main test" [...args: string] {
  cd (spleen--root)
  ^cargo test ...$args
}

# Run bacon (watch mode).
def "main watch" [] {
  cd (spleen--root)
  ^bacon
}

# Run an example in hot-reload mode (default: demo1).
def "main example" [
  name: string@spleen-complete-examples = "demo1",
  engine: string@spleen-complete-engines = "",
  --overlay  # Transparent layer-shell overlay (desktop_glass, prism_only, wire_grain, compositor_warp).
] {
  cd (spleen--root)
  if $overlay {
    ^./scripts/example.sh $name $engine --overlay
  } else {
    ^./scripts/example.sh $name $engine
  }
}

# Run the desktop-glass overlay with a hot-reloadable view.
def "main desktop-glass" [] {
  cd (spleen--root)
  ^./scripts/desktop-glass-hot.sh
}

# Rebuild riode-glass.so and reload it into the running Hyprland:
# unload first (tolerated when not loaded), then load. Absolute path
# so it works regardless of CWD or $HOME layout.
def "main load-plugin" [] {
  cd (spleen--root)
  cd plugins/riode-glass
  ^make
  let so = (spleen--root | path join plugins riode-glass riode-glass.so)
  do { ^hyprctl plugin unload $so } | ignore
  ^hyprctl plugin load $so
  ^systemctl --user restart xwayland-satellite.service
}

# Run a standalone overlay app (UI lives next to the app, e.g. apps/dock/src).
export def spleen-complete-overlays [] {
  ["dock"]
}

def "main apps" [
  app: string@spleen-complete-overlays = "dock",
] {
  cd (spleen--root)
  ^./scripts/overlay.sh $app
}

# Install dependencies in release folder. Runs first for release tasks.
def spleen-release-prepare [] {
  cd (spleen--root | path join scripts release)
  ^bun install
}

# Generate release commit and git tags.
def "main relacher" [] {
  spleen-release-prepare
  ^bun ./scripts/release/release.ts
}

# Generate release patch for github and deploy changes.
def "main release-github" [] {
  spleen-release-prepare
  ^bun ./scripts/release/release.ts
}

# --- Interactive import surface -------------------------------------------
export def example [
  name: string@spleen-complete-examples = "demo1",
  engine: string@spleen-complete-engines = "",
  --overlay
] {
  if $overlay {
    main example $name $engine --overlay
  } else {
    main example $name $engine
  }
}

export def desktop-glass [] {
  main desktop-glass
}

export def apps [
  app: string = "dock",
] {
  main apps $app
}

export def build [...args: string] {
  main build ...$args
}

export def test [...args: string] {
  main test ...$args
}

export def watch [] {
  main watch
}

export def relacher [] {
  main relacher
}

export def release-github [] {
  main release-github
}
