use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../../assets");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR must be set by Cargo"));
    let profile_dir = out_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("OUT_DIR must be under target/<profile>/build/<package>/out");

    let assets_dir = profile_dir.join("assets");
    if assets_dir.exists() {
        fs_extra::dir::remove(&assets_dir).unwrap();
    }

    fs_extra::dir::copy(
        "../../assets",
        profile_dir,
        &fs_extra::dir::CopyOptions {
            copy_inside: true,
            ..Default::default()
        },
    )
    .unwrap();
}
