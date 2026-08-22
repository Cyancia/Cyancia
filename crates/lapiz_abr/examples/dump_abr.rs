use std::{
    fs::{File, create_dir_all},
    io::Write,
    path::PathBuf,
};

use anyhow::bail;
use clap::Parser;

#[derive(Parser)]
struct Args {
    input: PathBuf,

    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(long)]
    dump_images: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let bytes = std::fs::read(&args.input)?;
    let abr = lapiz_abr::Abr::parse(&bytes)?;

    if let Some(output_dir) = &args.output {
        if output_dir.exists() && !output_dir.is_dir() {
            bail!("output must be a dir")
        }
        create_dir_all(output_dir)?;

        let mut desc = File::create(output_dir.join("desc"))?;
        write!(desc, "{:?}", abr)?;

        if args.dump_images {
            let patt_base = output_dir.join("patt");
            for (i, patt) in abr.patterns.into_iter().enumerate() {
                patt.as_image()?
                    .save(patt_base.join(format!("{}_{}.png", i, patt.name)))?;
            }

            let samp_base = output_dir.join("samp");
            for (i, samp) in abr.samples.into_iter().enumerate() {
                samp.as_image()?
                    .into_dynamic()
                    .save(samp_base.join(format!("{}_{}.png", i, samp.id)))?;
            }
        }
    } else {
        print!("{:?}", abr);
    }

    Ok(())
}
