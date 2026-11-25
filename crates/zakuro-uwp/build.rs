//! links recompiled code into the app when SMASH3DS_RECOMP_DIR names a
//! `3dsrecomp port` code folder, and gives the main thread, which runs the
//! game's code, the 8 MiB stack it has on Linux.

use std::env::var;
use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(smash_linked)");
    println!("cargo:rerun-if-env-changed=SMASH3DS_RECOMP_DIR");
    let family = var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let env = var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if let Some(dir) = var("SMASH3DS_RECOMP_DIR").ok().filter(|dir| !dir.is_empty()) {
        let archive = Path::new(&dir).join("librecomp.a");
        if !archive.is_file() {
            panic!("SMASH3DS_RECOMP_DIR is set, but {} is not there. Point it at the `code` folder `3dsrecomp port` wrote.", archive.display());
        }
        println!("cargo:rerun-if-changed={}", archive.display());
        if env == "msvc" {
            // MSVC's linker looks for recomp.lib, the same archive by that name
            let out = var("OUT_DIR").unwrap();
            std::fs::copy(&archive, Path::new(&out).join("recomp.lib")).expect("copy librecomp.a");
            println!("cargo:rustc-link-search=native={out}");
        } else {
            println!("cargo:rustc-link-search=native={dir}");
        }
        println!("cargo:rustc-link-lib=static=recomp");
        if family == "unix" {
            println!("cargo:rustc-link-lib=m");
        }
        println!("cargo:rustc-cfg=smash_linked");
    }

    if family == "windows" {
        let stack = 8 << 20;
        match env.as_str() {
            "msvc" => println!("cargo:rustc-link-arg-bins=/STACK:{stack}"),
            _ => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{stack}"),
        }
    }
}
