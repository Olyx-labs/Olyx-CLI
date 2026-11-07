fn main() {
    let tests = "../private-tests/usage_cost.rs";
    println!("cargo:rustc-check-cfg=cfg(private_regression_tests)");
    println!("cargo:rerun-if-changed={tests}");
    if std::path::Path::new(tests).is_file() {
        println!("cargo:rustc-cfg=private_regression_tests");
    }
}
