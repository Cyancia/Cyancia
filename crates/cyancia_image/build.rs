fn main() {
    println!("cargo:rerun-if-changed=src/shaders");

    wesl::PkgBuilder::new("image")
        .scan_root("src/shaders")
        .unwrap()
        .validate()
        .unwrap()
        .build_artifact()
        .unwrap();

    let compiler = wesl::Wesl::new("src/shaders");
    compiler.build_artifact(&"package::scan_pixels".parse().unwrap(), "scan_pixels");
}
