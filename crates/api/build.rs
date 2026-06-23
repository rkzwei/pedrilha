//! Build script for gem-finder-api.
//!
//! Automatically builds the Leptos WASM frontend (CSS + trunk) before the API
//! compiles, so `cargo run -p gem-finder-api` is a single command that does
//! everything.
//!
//! Set SKIP_TRUNK_BUILD=1 to bypass (Docker API stage, CI backend job, or when
//! you've already built the frontend manually).

use std::{path::Path, process::Command};

fn main() {
    if std::env::var("SKIP_TRUNK_BUILD").is_ok() {
        println!("cargo:warning=SKIP_TRUNK_BUILD set — skipping frontend build");
        return;
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // crates/api → crates → workspace root
    let workspace = Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let frontend_dir = workspace.join("crates/frontend");

    // Re-run only when these change (keeps incremental builds fast)
    for path in &[
        frontend_dir.join("src"),
        frontend_dir.join("index.html"),
        frontend_dir.join("assets/input.css"),
        frontend_dir.join("tailwind.config.js"),
    ] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rerun-if-env-changed=SKIP_TRUNK_BUILD");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());

    // ── Step 1: Tailwind CSS ──────────────────────────────────────────────────
    let css_ok = Command::new("npm")
        .args(["run", "build:css"])
        .current_dir(&frontend_dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !css_ok {
        println!("cargo:warning=CSS build skipped (npm unavailable or failed)");
    }

    // ── Step 2: Trunk WASM build ──────────────────────────────────────────────
    let mut trunk_args = vec!["build"];
    if profile == "release" {
        trunk_args.push("--release");
    }

    let trunk_ok = Command::new("trunk")
        .args(&trunk_args)
        .current_dir(workspace)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !trunk_ok {
        println!(
            "cargo:warning=Frontend build skipped (trunk unavailable or failed). \
             Install with: cargo install trunk"
        );
    }
}
