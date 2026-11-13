// Debug CLI for the glue: reads a custom effect module (or a complete
// wire module with --complete), prints the GLSL ES 3.00 fragment shader
// the plugin would compile, or the transpile error on stderr (exit 1).
// This is the human-operated probe when a stage refuses to latch or
// link — feed it exactly what rode the wire.

use riode_shader_glue::transpile;

const ABI_HEADER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/core/riode-shader/src/effect/effect_abi.wgsl"
));

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (complete, path) = match args.as_slice() {
        [flag, rest] if flag == "--complete" => (true, rest.as_str()),
        [rest] => (false, rest.as_str()),
        _ => {
            eprintln!("usage: riode-shader-glue [--complete] <file.wgsl | ->");
            std::process::exit(2);
        }
    };
    let input = if path == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .expect("read stdin");
        s
    } else {
        std::fs::read_to_string(path).expect("read file")
    };
    let module = if complete {
        input
    } else {
        format!("{ABI_HEADER}\n{input}")
    };
    match transpile(&module) {
        Ok(glsl) => print!("{glsl}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
