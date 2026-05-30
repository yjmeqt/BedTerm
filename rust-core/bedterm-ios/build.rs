//! Compile-time codegen: run cbindgen across this crate *and* its
//! `bedterm-core` + `bedterm-app` path-deps (whitelisted in `cbindgen.toml`),
//! producing the single C header `include/bedterm_ios.h` consumed by
//! `scripts/build-rust-xcframework.sh`.
//!
//! Locale table generation (`$OUT_DIR/locale_tables.rs`) has moved to
//! `bedterm-app/build.rs` alongside the `l10n` module.

use std::{env, fs, path::PathBuf};

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // ── cbindgen: emit `include/bedterm_ios.h` covering this crate + the
    //    whitelisted `bedterm-core` + `bedterm-app` path-deps (see `cbindgen.toml`).
    let header_out = PathBuf::from(&crate_dir)
        .join("include")
        .join("bedterm_ios.h");
    fs::create_dir_all(header_out.parent().unwrap()).unwrap();

    // Rerun the cbindgen step when any FFI source changes, in any of the
    // three crates. Globs aren't supported, so list the known FFI files
    // explicitly.
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/ffi/mod.rs");
    println!("cargo:rerun-if-changed=src/ffi/connect_form.rs");
    println!("cargo:rerun-if-changed=src/ffi/connect_form_vm.rs");
    println!("cargo:rerun-if-changed=src/ffi/host_keys.rs");
    println!("cargo:rerun-if-changed=src/ffi/hosts.rs");
    println!("cargo:rerun-if-changed=src/ffi/onboarding.rs");
    println!("cargo:rerun-if-changed=src/ffi/settings.rs");
    println!("cargo:rerun-if-changed=src/ffi/vc.rs");
    println!("cargo:rerun-if-changed=src/ffi/view.rs");
    println!("cargo:rerun-if-changed=src/terminal_session/mod.rs");
    // bedterm-core FFI files
    println!("cargo:rerun-if-changed=../bedterm-core/src/ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/lib.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/blocks_ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/blocks.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/dcs.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/renderer/ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/renderer/block_list_ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/snapshot.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/term.rs");
    // bedterm-app FFI files (moved from bedterm-ios)
    println!("cargo:rerun-if-changed=../bedterm-app/src/lib.rs");
    println!("cargo:rerun-if-changed=../bedterm-app/src/l10n.rs");
    println!("cargo:rerun-if-changed=../bedterm-app/src/shell_integration.rs");
    println!("cargo:rerun-if-changed=../bedterm-app/src/net_util.rs");
    println!("cargo:rerun-if-changed=../bedterm-app/src/ssh_bridge.rs");

    let cfg = cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml"))
        .expect("read bedterm-ios/cbindgen.toml");
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(cfg)
        .generate()
        .expect("cbindgen failed")
        .write_to_file(&header_out);
}
