use anyhow::{Context, Error, Result};
use clap::Parser;

mod args;
mod compressor;
mod utils;

fn prepare_paths(args: &args::Args) -> Result<(std::path::PathBuf, std::path::PathBuf), Error> {
    let input = std::path::Path::new(&args.input).to_path_buf();

    if !input.exists() {
        return Result::Err(Error::msg(format!(
            "Input path does not exist: {:?}",
            input
        )));
    }

    let output = match args.command {
        args::Command::C => {
            let output = match (&args.output, args.compress_type) {
                (Some(output), _) => std::path::Path::new(&output).to_path_buf(),
                (_, args::CompressType::TARZSTD) => input.with_extension("tar.zst"),
            };
            if output.exists() {
                std::fs::remove_file(&output)
                    .with_context(|| format!("Failed to remove file: {:?}", &output))?;
            }
            output
        }
        args::Command::X => {
            if !input.is_file() {
                return Result::Err(Error::msg(format!("Input path is not a file: {:?}", input)));
            }
            let output = match (&args.output, args.compress_type) {
                (Some(output), _) => std::path::Path::new(&output).to_path_buf(),
                (_, args::CompressType::TARZSTD) => input.parent().unwrap().to_path_buf(),
            };
            if output.exists() && !output.is_dir() {
                return Result::Err(Error::msg(format!(
                    "Output path is not a directory: {:?}",
                    output
                )));
            }
            output
        }
    };

    Ok((input, output))
}

fn main() -> Result<()> {
    let args = args::Args::parse();

    let start = std::time::Instant::now();

    match (args.command, args.compress_type) {
        (args::Command::C, args::CompressType::TARZSTD) => {
            let (input, output) =
                prepare_paths(&args).with_context(|| format!("Failed to prepare paths"))?;
            if args.log_level >= 1 {
                println!("Compress from: {:?}", input);
                println!("Compress to  : {:?}", output);
            }
            let output_writer = std::fs::File::create(&output)
                .with_context(|| format!("Failed to create file: {:?}", &output))?;

            compressor::tar_zstd(&args, &input, output_writer).with_context(|| {
                format!(
                    "Failed to create tar zstd from: {:?} to: {:?}",
                    input, output
                )
            })?;

            let elapsed = start.elapsed();
            let size = std::fs::metadata(&output)
                .with_context(|| format!("Failed to get metadata for: {:?}", &output))?
                .len();
            let speed = size as f64 / elapsed.as_secs_f64();
            if args.log_level >= 1 {
                println!(
                    "Compress end : {}, {}, {}/s",
                    utils::readable_bytes(size),
                    utils::readable_elapse(elapsed.as_secs_f64()),
                    utils::readable_bytes(speed as u64)
                );
            }
        }
        (args::Command::X, args::CompressType::TARZSTD) => {
            let (input, output) = prepare_paths(&args)
                .with_context(|| format!("Failed to prepare paths for: {:?}", args))?;
            if args.log_level >= 1 {
                println!("Decompress from: {:?}", input);
                println!("Decompress to  : {:?}", output);
            }
            let input_reader = std::fs::File::open(&input)
                .with_context(|| format!("Failed to open file: {:?}", &input))?;
            compressor::untar_zstd(&args, &input_reader, &output).with_context(|| {
                format!(
                    "Failed to decompress tar zstd from: {:?} to: {:?}",
                    input, output
                )
            })?;

            let elapsed = start.elapsed();
            let size = std::fs::metadata(&input)
                .with_context(|| format!("Failed to get metadata for: {:?}", &input))?
                .len();
            let speed = size as f64 / elapsed.as_secs_f64();
            if args.log_level >= 1 {
                println!(
                    "DeCompress end : {}, {}, {}/s",
                    utils::readable_bytes(size),
                    utils::readable_elapse(elapsed.as_secs_f64()),
                    utils::readable_bytes(speed as u64)
                );
            }
        }
    }

    Ok(())
}
