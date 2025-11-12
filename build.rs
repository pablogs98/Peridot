use std::path::Path;

fn main() {
    if Path::new("/usr/local/rust/Cargo.toml").exists() {
        println!("cargo:rustc-cfg=feature=\"geds_rs\"");
    }
}
