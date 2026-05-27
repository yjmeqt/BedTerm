//! Compile-time codegen: read `BedTerm/Localizable.xcstrings` and emit a
//! flat Rust lookup table (`LOCALE_TABLES`) into `$OUT_DIR/locale_tables.rs`.
//! The xcstrings file is the single source of truth — `t(key)` at runtime
//! does no parsing, no FFI, just a slice scan.

use serde::Deserialize;
use std::{collections::BTreeMap, env, fs, path::Path};

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
    // sits at rust-core/bedterm_ios/ — walk up two parents.
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
}
