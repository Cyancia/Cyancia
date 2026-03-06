fn main() {
    wesl::PkgBuilder::new("image")
        .scan_root("src/shaders")
        .unwrap()
        .validate()
        .unwrap()
        .build_artifact()
        .unwrap();
}
