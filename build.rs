//! Embeds every file in `assets/templates/` (shipped project templates and
//! their background images) so adding a template needs no code change.

use std::{env, fs, path::Path};

fn main() {
    let dir = Path::new("assets/templates");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut files: Vec<_> = fs::read_dir(dir)
        .expect("assets/templates exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut out = String::from("/// Shipped templates and their images, as (asset path, bytes).\n");
    out.push_str("pub const TEMPLATE_FILES: &[(&str, &[u8])] = &[\n");
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let name = path.file_name().unwrap().to_string_lossy();
        let abs = fs::canonicalize(path).unwrap();
        out.push_str(&format!("    (\"templates/{name}\", include_bytes!({:?})),\n", abs.display().to_string()));
    }
    out.push_str("];\n");

    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("templates.rs");
    fs::write(dest, out).unwrap();
}
