use wesl::Wesl;

fn main() {
    let mut compiler = Wesl::new("shader");
    compiler.add_package(&cyancia_color::color::PACKAGE);
    compiler.build_artifact(&"package::gradient".parse().unwrap(), "gradient");
}
