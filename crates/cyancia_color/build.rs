use wesl::PkgBuilder;

fn main() {
    println!("cargo:rerun-if-changed=shaders");

    PkgBuilder::new("color")
        .scan_root("shaders")
        .unwrap()
        .validate()
        .inspect_err(|e| panic!("{}", e))
        .unwrap()
        .build_artifact()
        .unwrap();
}
