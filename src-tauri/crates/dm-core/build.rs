// Embeds every remedies/*.json file (sorted) so the library is data: new
// remedies need no code change. library.rs parses and checks them.
use std::fmt::Write;

fn main() {
    println!("cargo:rerun-if-changed=remedies");
    let mut files: Vec<_> = std::fs::read_dir("remedies")
        .expect("remedies folder")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json") && n != "targets.json")
        .collect();
    files.sort();
    let dir = std::env::current_dir().expect("cwd").join("remedies");
    let mut out = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for f in &files {
        let path = dir.join(f);
        writeln!(out, "    ({f:?}, include_str!({:?})),", path.to_string_lossy()).unwrap();
    }
    out.push_str("];\n");
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("remedy_files.rs");
    std::fs::write(dest, out).expect("write remedy_files.rs");
}
