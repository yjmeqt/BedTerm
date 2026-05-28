//! Compile-time codegen:
//!
//! 1. Read `BedTerm/Localizable.xcstrings` and emit a flat Rust lookup
//!    table (`LOCALE_TABLES`) into `$OUT_DIR/locale_tables.rs`. The
//!    xcstrings file is the single source of truth — `t(key)` at runtime
//!    does no parsing, no FFI, just a slice scan.
//!
//! 2. Run cbindgen across this crate *and* its `bedterm-core` path-dep
//!    (whitelisted in `cbindgen.toml`), producing the single C header
//!    `include/bedterm_ios.h` consumed by `scripts/build-rust-xcframework.sh`.
//!    Swift sees both `bt_term_*` (core) and `bt_ios_*` (ui) symbols from
//!    one generator pass.

use serde::Deserialize;
use std::{collections::BTreeMap, env, fs, path::Path, path::PathBuf};

#[derive(Deserialize)]
struct XcStrings {
    strings: BTreeMap<String, Entry>,
}
#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    localizations: BTreeMap<String, Loc>,
}
#[derive(Deserialize)]
struct Loc {
    #[serde(rename = "stringUnit")]
    unit: Option<StringUnit>,
}
#[derive(Deserialize)]
struct StringUnit {
    value: String,
}

fn main() {
    // The xcstrings file lives at the repo root under BedTerm/. The crate
    // sits at rust-core/bedterm-ios/ — walk up two parents.
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let xcstrings = Path::new(&crate_dir)
        .join("..")
        .join("..")
        .join("BedTerm")
        .join("Localizable.xcstrings");
    println!("cargo:rerun-if-changed={}", xcstrings.display());

    let raw = fs::read_to_string(&xcstrings)
        .unwrap_or_else(|e| panic!("read {}: {e}", xcstrings.display()));
    let parsed: XcStrings =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse xcstrings: {e}"));

    // Pivot: locale -> [(key, value)]. Use BTreeMap for stable output.
    let mut by_locale: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (key, entry) in &parsed.strings {
        for (loc, l) in &entry.localizations {
            if let Some(u) = &l.unit {
                by_locale
                    .entry(loc.clone())
                    .or_default()
                    .push((key.clone(), u.value.clone()));
            }
        }
    }
    for v in by_locale.values_mut() {
        v.sort_by(|a, b| a.0.cmp(&b.0));
    }

    let total_keys = parsed.strings.len();
    let total_locales = by_locale.len();
    println!(
        "cargo:warning=bedterm_ios l10n: parsed {} keys across {} locales",
        total_keys, total_locales
    );

    let mut out =
        String::from("// AUTO-GENERATED from BedTerm/Localizable.xcstrings — do not edit.\n");
    out.push_str("pub static LOCALE_TABLES: &[(&str, &[(&str, &str)])] = &[\n");
    for (loc, pairs) in &by_locale {
        out.push_str(&format!("    (\"{loc}\", &[\n"));
        for (k, v) in pairs {
            out.push_str(&format!("        ({k:?}, {v:?}),\n"));
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");

    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("locale_tables.rs");
    fs::write(&dest, out).unwrap_or_else(|e| panic!("write {}: {e}", dest.display()));

    // ── cbindgen: emit `include/bedterm_ios.h` covering this crate + the
    //    whitelisted `bedterm-core` path-dep (see `cbindgen.toml`). The
    //    staging script copies it into the xcframework slice and the
    //    xcframework Headers/ directory via its module.modulemap.
    let header_out = PathBuf::from(&crate_dir)
        .join("include")
        .join("bedterm_ios.h");
    fs::create_dir_all(header_out.parent().unwrap()).unwrap();

    // Rerun the cbindgen step when any FFI source changes, in either
    // crate. Globs aren't supported, so list the known FFI files
    // explicitly — they're stable / small in number.
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/l10n.rs");
    println!("cargo:rerun-if-changed=src/shell_integration.rs");
    println!("cargo:rerun-if-changed=assets/bedterm-integration.sh");
    println!("cargo:rerun-if-changed=src/russh_client.rs");
    println!("cargo:rerun-if-changed=src/ssh_bridge.rs");
    println!("cargo:rerun-if-changed=src/ffi/mod.rs");
    println!("cargo:rerun-if-changed=src/ffi/connect_form.rs");
    println!("cargo:rerun-if-changed=src/ffi/host_keys.rs");
    println!("cargo:rerun-if-changed=src/ffi/hosts.rs");
    println!("cargo:rerun-if-changed=src/ffi/onboarding.rs");
    println!("cargo:rerun-if-changed=src/ffi/settings.rs");
    println!("cargo:rerun-if-changed=src/ffi/vc.rs");
    println!("cargo:rerun-if-changed=src/ffi/view.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/lib.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/blocks_ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/blocks.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/dcs.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/renderer/ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/renderer/block_list_ffi.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/snapshot.rs");
    println!("cargo:rerun-if-changed=../bedterm-core/src/term.rs");

    let cfg = cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml"))
        .expect("read bedterm-ios/cbindgen.toml");
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(cfg)
        .generate()
        .expect("cbindgen failed")
        .write_to_file(&header_out);
}
