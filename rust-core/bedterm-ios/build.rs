//! Run cbindgen across this crate + `bedterm-app`, producing
//! `include/bedterm_ios.h` consumed by `scripts/build-rust-xcframework.sh`.

use std::{env, fs, path::PathBuf};

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let header_out = PathBuf::from(&crate_dir)
        .join("include")
        .join("bedterm_ios.h");
    fs::create_dir_all(header_out.parent().unwrap()).unwrap();

    // Rerun when cbindgen config or any FFI surface changes.
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/root_coordinator.rs");
    println!("cargo:rerun-if-changed=src/ffi/mod.rs");
    println!("cargo:rerun-if-changed=src/ffi/settings.rs");
    // bedterm-app FFI surface
    println!("cargo:rerun-if-changed=../bedterm-app/src/lib.rs");
    println!("cargo:rerun-if-changed=../bedterm-app/src/l10n.rs");

    let cfg = cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml"))
        .expect("read bedterm-ios/cbindgen.toml");
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(cfg)
        .generate()
        .expect("cbindgen failed")
        .write_to_file(&header_out);
}
