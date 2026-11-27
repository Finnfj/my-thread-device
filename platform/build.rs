//! Linker scripts for the on-device test binary (`cargo xtask hil`, ADR 0011).
//!
//! The Platform is a library, so the only binary built from this crate is its
//! unit-test binary; a Firmware links the Platform with its own build script.

fn main() {
    // embedded-test's script first: linkall.x must be the last one.
    println!("cargo::rustc-link-arg=-Tembedded-test.x");
    println!("cargo::rustc-link-arg=-Tlinkall.x");
    // embedded-test's macros check `cfg(rust_analyzer)`.
    println!("cargo::rustc-check-cfg=cfg(rust_analyzer)");
}
