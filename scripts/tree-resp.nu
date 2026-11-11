#!/usr/bin/env nu
# usage: ./tree-resp.nu [root] [--all]   (default root: crates)

const PREFIX = "// Responsibility: "
const SKIP = ["examples", "tests"]

def comment [p: path] {
    let line = (try { open --raw $p | lines | get 0? | default "" } catch { "" })
    if ($line | str starts-with $PREFIX) {
        $line | str replace $PREFIX "" | str trim
    } else { "" }
}

def walk [dir: path, prefix: string, all: bool] {
    let entries = (ls $dir | where {|e| ($e.name | path basename) != "target" } | sort-by name)
    let n = ($entries | length)
    $entries | enumerate | each {|it|
        let e = $it.item
        let last = ($it.index == $n - 1)
        let base = ($e.name | path basename)
        let is_dir = ($e.type == "dir")
        let branch = if $last { "└── " } else { "├── " }
        let pad = if $last { "    " } else { "│   " }
        let collapsed = ($is_dir and (not $all) and ($base in $SKIP))
        let name = if $collapsed { $"($base)/..." } else if $is_dir { $"($base)/" } else { $base }
        let row = [{
            left: $"($prefix)($branch)($name)"
            note: (if $is_dir { "" } else { comment $e.name })
        }]
        if $is_dir and (not $collapsed) {
            $row | append (walk $e.name $"($prefix)($pad)" $all)
        } else { $row }
    } | flatten
}

def main [root: path = "crates", --all (-a)] {
    let root = ($root | str trim --right --char "/")
    let rows = ([{ left: $"($root)/", note: "" }] | append (walk $root "" $all))
    $rows
    | each {|r|
        if $r.note == "" { $r.left } else { $"($r.left) # ($r.note)" }
    }
    | str join "\n"
}
