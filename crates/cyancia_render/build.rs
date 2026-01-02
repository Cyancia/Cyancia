fn main() {
    wesl::Wesl::new("src/shaders").build_artifact(
        &"package::fullscreen_vertex".parse().unwrap(),
        "fullscreen_vertex",
    );

    wesl::PkgBuilder::new("render")
        .scan_root("src/shaders")
        .unwrap()
        .validate()
        .unwrap()
        .build_artifact()
        .unwrap();
}
