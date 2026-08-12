use wesl::Wesl;

fn main() {
    println!("cargo:rerun-if-changed=shaders");

    let mut compiler = Wesl::new("shaders");
    compiler.add_package(&lapiz_image::image::PACKAGE);

    compiler.build_artifact(
        &"package::compose_stroke_preview".parse().unwrap(),
        "compose_stroke_preview",
    );
}
