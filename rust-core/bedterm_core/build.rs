use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = PathBuf::from(&crate_dir)
        .join("include")
        .join("bedterm_core.h");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/snapshot.rs");
    println!("cargo:rerun-if-changed=src/term.rs");
    println!("cargo:rerun-if-changed=src/mock_tty/ffi.rs");
    println!("cargo:rerun-if-changed=src/mock_tty/mod.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml")).unwrap())
        .generate()
        .expect("cbindgen failed")
        .write_to_file(&out);
}
