fn main() {
    for dst in ["target/debug/assets", "target/release/assets"] {
        fs_extra::dir::remove(dst).unwrap();
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
