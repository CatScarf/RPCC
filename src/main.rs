use anyhow::{Context, Error, Result};
use clap::Parser;

mod args;
mod tests;
mod utils;
mod zip;
mod zstd;

fn prepare_paths(args: &args::Args) -> Result<(std::path::PathBuf, std::path::PathBuf), Error> {
    let input = std::path::Path::new(&args.input).to_path_buf();

    if !input.exists() {
        return Result::Err(Error::msg(format!(
            "Input path does not exist: {:?}",
            input
        )));
    }

    let msg;
    let output = match args.command {
        args::Command::C => {
            msg = "Compress";
            let output = match (&args.output, args.compress_type) {
                (Some(output), _) => std::path::Path::new(&output).to_path_buf(),
                (_, args::CompressType::TARZSTD) => input.with_extension("tar.zst"),
                (_, args::CompressType::ZIP) => input.with_extension("zip"),
            };
            if output.exists() {
                std::fs::remove_file(&output)
                    .with_context(|| format!("Failed to remove file: {:?}", &output))?;
            }
            output
        }
        args::Command::X => {
            msg = "Decompress";
            if !input.is_file() {
                return Result::Err(Error::msg(format!("Input path is not a file: {:?}", input)));
            }
            let output = match (&args.output, args.compress_type) {
                (Some(output), _) => std::path::Path::new(&output).to_path_buf(),
                (_, args::CompressType::TARZSTD | args::CompressType::ZIP) => {
                    input.parent().unwrap().to_path_buf()
                }
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

    if args.log_level >= 1 {
        println!("{} from: {:?}", msg, input);
        println!("{} to  : {:?}", msg, output);
    }

    Ok((input, output))
}

fn after_compress(start: std::time::Instant, output: &std::path::Path, args: &args::Args) {
    let elapsed = start.elapsed();
    let size = if let Ok(meta) = std::fs::metadata(&output) {
        meta.len()
    } else {
        println!("Failed to get metadata for: {:?}", &output);
        return;
    };

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

fn after_decompress(start: std::time::Instant, input: &std::path::Path, args: &args::Args) {
    let elapsed = start.elapsed();
    let size = if let Ok(meta) = std::fs::metadata(&input) {
        meta.len()
    } else {
        println!("Failed to get metadata for: {:?}", &input);
        return;
    };
    let speed = size as f64 / elapsed.as_secs_f64();
    if args.log_level >= 1 {
        println!(
            "Decompress end : {}, {}, {}/s",
            utils::readable_bytes(size),
            utils::readable_elapse(elapsed.as_secs_f64()),
            utils::readable_bytes(speed as u64)
        );
    }
}

fn main() -> Result<()> {
    let args = args::Args::parse();

    let start = std::time::Instant::now();

    match (args.command, args.compress_type) {
        (args::Command::C, args::CompressType::TARZSTD) => {
            let (input, output) = prepare_paths(&args)?;
            let mut output_writer = std::fs::File::create(&output)
                .with_context(|| format!("Failed to create file: {:?}", &output))?;

            zstd::tar_zstd(
                &input,
                &mut output_writer,
                args.compress_level.unwrap_or(3),
                args.no_long_distance_matching,
                args.small_file_size,
                args.log_level,
            )
            .with_context(|| {
                format!(
                    "Failed to create tar zstd from: {:?} to: {:?}",
                    input, output
                )
            })?;

            after_compress(start, &output, &args);
        }
        (args::Command::X, args::CompressType::TARZSTD) => {
            let (input, output) = prepare_paths(&args)?;
            let mut input_reader = std::fs::File::open(&input)
                .with_context(|| format!("Failed to open file: {:?}", &input))?;
            zstd::untar_zstd(&mut input_reader, &output).with_context(|| {
                format!(
                    "Failed to decompress tar zstd from: {:?} to: {:?}",
                    input, output
                )
            })?;

            after_decompress(start, &input, &args);
        }
        (args::Command::C, args::CompressType::ZIP) => {
            let (input, output) = prepare_paths(&args)?;
            let mut output_writer = std::fs::File::create(&output)
                .with_context(|| format!("Failed to create file: {:?}", &output))?;
            zip::zip(&input, &mut output_writer, args.log_level).unwrap();
            after_compress(start, &output, &args);
        }
        (args::Command::X, args::CompressType::ZIP) => {
            let (input, output) = prepare_paths(&args)?;
            let mut input_reader = std::fs::File::open(&input)
                .with_context(|| format!("Failed to open file: {:?}", &input))?;
            zip::unzip(&mut input_reader, &output).with_context(|| {
                format!(
                    "Failed to decompress zip from: {:?} to: {:?}",
                    input, output
                )
            })?;

            after_decompress(start, &input, &args);
        }
    }

    Ok(())
}
