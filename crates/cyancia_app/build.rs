use std::path::Path;

fn main() {
    for dst in ["../../target/debug", "../../target/release"] {
        fs_extra::dir::remove(Path::new(dst).join("assets")).unwrap();
        fs_extra::dir::copy(
            "../../assets",
            dst,
            &fs_extra::dir::CopyOptions {
                copy_inside: true,
                ..Default::default()
            },
        )
        .unwrap();
    }
}
