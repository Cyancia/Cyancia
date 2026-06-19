use wesl::Wesl;

fn main() {
    println!("cargo:rerun-if-changed=shaders");

    let mut compiler = Wesl::new("shaders");
    compiler.add_package(&cyancia_image::image::PACKAGE);
    compiler.add_package(&cyancia_render::render::PACKAGE);

    compiler.build_artifact(
        &"package::thresholding.wesl".parse().unwrap(),
        "thresholding",
    );
    compiler.build_artifact(&"package::seed_mode.wesl".parse().unwrap(), "seed_mode");
    compiler.build_artifact(&"package::grow.wesl".parse().unwrap(), "grow");
    compiler.build_artifact(&"package::ccl.wesl".parse().unwrap(), "ccl");
    compiler.build_artifact(&"package::composite.wesl".parse().unwrap(), "composite");
    compiler.build_artifact(&"package::fxaa.wesl".parse().unwrap(), "fxaa");
    compiler.build_artifact(
        &"package::close_gap_and_feather.wesl".parse().unwrap(),
        "close_gap_and_feather",
    );
}
