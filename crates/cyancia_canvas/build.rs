fn main() {
    wesl::Wesl::new("src/shaders")
        .build_artifact(&"package::canvas_render".parse().unwrap(), "canvas_render");

    wesl::Wesl::new("src/shaders")
        .add_package(&cyancia_render::render::PACKAGE)
        .build_artifact(
            &"package::canvas_present".parse().unwrap(),
            "canvas_present",
        );
}
