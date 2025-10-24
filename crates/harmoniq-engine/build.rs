fn main() {
    if std::env::var("CARGO_CFG_TEST").is_ok() {
        println!("cargo:rustc-cfg=deny_alloc_in_rt");
    }
    println!("cargo:rustc-check-cfg=cfg(deny_alloc_in_rt)");
}
