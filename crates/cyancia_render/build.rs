fn main() {
    println!("cargo:rerun-if-changed=src/shaders/hash.wesl");
    println!("cargo:rerun-if-changed=src/shaders/math.wesl");

    wesl::PkgBuilder::new("render")
        .scan_root("src/shaders")
        .unwrap()
        .validate()
        .unwrap()
        .build_artifact()
        .unwrap();
}
