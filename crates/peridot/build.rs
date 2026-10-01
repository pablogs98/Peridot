use std::process::Command;

/// Records the compiler identity so a plugin can be checked against the runtime it will be
/// loaded into. Contexts cross the boundary as ordinary Rust types (`Box<dyn DelegatingWasiCtx>`,
/// boxed futures), which have no stable ABI, so a mismatched toolchain is undefined behaviour
/// rather than a load error. This is what makes it detectable.
fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = Command::new(rustc)
        .arg("-vV")
        .output()
        .expect("failed to run rustc to determine the plugin ABI fingerprint");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut version = "unknown";
    let mut host = "unknown";
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("release: ") {
            version = rest.trim();
        } else if let Some(rest) = line.strip_prefix("host: ") {
            host = rest.trim();
        }
    }

    println!("cargo:rustc-env=PERIDOT_RUSTC_VERSION={version}");
    println!("cargo:rustc-env=PERIDOT_TARGET_TRIPLE={host}");
    println!("cargo:rerun-if-env-changed=RUSTC");
    println!("cargo:rerun-if-changed=build.rs");
}
