fn main() {
    println!("cargo:rerun-if-changed=src/shaders");

    let mut shaders = wesl::Wesl::new("src/shaders");
    shaders
        .add_package(&lapiz_image::image::PACKAGE)
        .add_package(&lapiz_render::render::PACKAGE);

    shaders.build_artifact(
        &"package::canvas_present".parse().unwrap(),
        "canvas_present",
    );
}
