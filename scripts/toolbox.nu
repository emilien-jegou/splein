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
] {
  cd (spleen--root)
  ^./scripts/example.sh $name $engine
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
] {
  main example $name $engine
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
