use wesl::Wesl;

fn main() {
    println!("cargo:rerun-if-changed=shaders");

    let mut compiler = Wesl::new("shaders");
    compiler.add_package(&cyancia_image::image::PACKAGE);

    compiler.build_artifact(
        &"package::thresholding.wesl".parse().unwrap(),
        "thresholding",
    );
    compiler.build_artifact(
        &"package::debug_bit_mask.wesl".parse().unwrap(),
        "debug_bit_mask",
    );
    compiler.build_artifact(
        &"package::ccl.wesl".parse().unwrap(),
        "ccl",
    );
}
