use std::env;

fn main() {
    let target = env::var("CARGO_CFG_TARGET_OS").unwrap();

    #[cfg(target_env = "musl")]
    println!("cargo:rustc-link-arg=-Wl,-z,origin");

    if target == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../lib");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../lib");
    }
}
