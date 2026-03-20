fn main() {
    let mut shaders = wesl::Wesl::new("src/render");

    shaders.add_package(&cyancia_image::image::PACKAGE);

    shaders.build_artifact(
        &"package::brush_tile_allocation.wesl".parse().unwrap(),
        "brush_tile_allocation",
    );
}
