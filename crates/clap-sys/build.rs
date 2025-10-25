use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use bindgen::EnumVariation;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const HASH_FILE: &str = "clap-headers.sha256";

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let (clap_include, verify_hash) = match env::var("CLAP_HEADERS_DIR") {
        Ok(path) => (PathBuf::from(path), false),
        Err(_) => {
            let repo_include = manifest_dir
                .join("..")
                .join("..")
                .join("third_party")
                .join("clap")
                .join("include");

            if repo_include.exists() {
                (repo_include, true)
            } else {
                let bundled_include = manifest_dir.join("include");
                (bundled_include, true)
            }
        }
    };

    guard_header_hash(&manifest_dir, &clap_include, verify_hash);

    let mut builder = bindgen::Builder::default()
        .ctypes_prefix("cty")
        .use_core()
        .allowlist_recursively(true)
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        .layout_tests(false)
        .generate_inline_functions(true)
        .default_enum_style(EnumVariation::NewType {
            is_bitfield: false,
            is_global: false,
        })
        .clang_arg(format!("-I{}", clap_include.display()))
        .formatter(bindgen::Formatter::Rustfmt);

    let master_header = clap_include.join("clap/all.h");
    builder = builder.header(master_header.to_string_lossy());

    // Avoid pulling libc definitions that collide with cty
    builder = builder
        .blocklist_type("max_align_t")
        .blocklist_type("__uint128_t")
        .blocklist_type("__int128_t");

    let bindings = builder
        .generate()
        .expect("Unable to generate CLAP bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

fn guard_header_hash(manifest_dir: &Path, include_dir: &Path, verify_hash: bool) {
    if !include_dir.exists() {
        panic!(
            "CLAP headers directory missing at {include_dir:?}; set `CLAP_HEADERS_DIR` or ensure the bundled headers are present (run `git submodule update --init --recursive` or `cargo xtask regenerate-clap`).",
        );
    }

    if !verify_hash {
        return;
    }

    let mut hasher = Sha256::new();

    let mut header_paths: Vec<PathBuf> = WalkDir::new(include_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().is_file()
                && entry.path().extension().and_then(|ext| ext.to_str()) == Some("h")
        })
        .map(|entry| entry.path().to_path_buf())
        .collect();

    header_paths.sort();

    if header_paths.is_empty() {
        panic!(
            "No CLAP header files found under {include_dir:?}; ensure the CLAP headers are available (run `git submodule update --init --recursive` or regenerate them).",
        );
    }

    for path in header_paths {
        let mut file = File::open(&path).expect("Failed to open header");
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).expect("Failed to read header");
        hasher.update(&buf);
    }

    let digest = hasher.finalize();
    let digest_hex = hex::encode(digest);

    let hash_file = manifest_dir.join(HASH_FILE);
    let expected = fs::read_to_string(&hash_file)
        .unwrap_or_else(|_| panic!("{} missing; run `cargo xtask regenerate-clap`", HASH_FILE));

    let expected = expected.trim();
    if expected != digest_hex {
        panic!(
            "CLAP headers changed (expected {expected}, found {digest_hex}). Re-run bindings generation.",
        );
    }
}
