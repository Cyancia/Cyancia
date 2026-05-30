fn main() {
    println!("cargo:rerun-if-changed=src/shaders");

    let mut shaders = wesl::Wesl::new("src/shaders");
    shaders
        .add_package(&cyancia_image::image::PACKAGE)
        .add_package(&cyancia_render::render::PACKAGE);

    shaders.build_artifact(&"package::canvas_render".parse().unwrap(), "canvas_render");
    shaders.build_artifact(
        &"package::canvas_present".parse().unwrap(),
        "canvas_present",
    );
}
