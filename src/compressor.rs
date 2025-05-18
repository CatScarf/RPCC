use crate::args;
use anyhow::{Context, Error, Result};
use num_cpus;
use rayon::prelude::*;
use std::{io::Write, os::windows::fs::MetadataExt};

struct TarFileData {
    rel_path: std::path::PathBuf,
    file: std::fs::File,
    cursor: Option<(std::io::Cursor<Vec<u8>>, tar::Header)>,
}

struct TarWriter;

impl TarWriter {
    fn start(
        args: &args::Args,
        src_dir: &std::path::Path,
        tar_builder: &mut tar::Builder<impl Write>,
    ) -> Result<std::thread::JoinHandle<Result<(), Error>>, Error> {
        let (tx, rx) = std::sync::mpsc::sync_channel(100);

        let small_file_size = args.small_file_size.unwrap_or(0);
        let src_dir_buf = src_dir.to_path_buf();

        // Start the thread to process files in the directory

        let thread = std::thread::spawn(move || -> Result<(), Error> {
            let result = walkdir::WalkDir::new(&src_dir_buf)
                .into_iter()
                .enumerate()
                .par_bridge()
                .map(|(_, entry)| -> Result<(), Error> {
                    let entry = entry?;
                    TarWriter::send_tar_data(small_file_size, &src_dir_buf, &tx, entry)
                })
                .filter(|result| !result.is_ok())
                .collect::<Vec<_>>();

            if !result.is_empty() {
                return Result::Err(Error::msg(format!(
                    "Failed to process all files in directory {:?}: {:?}",
                    src_dir_buf, result
                )));
            }

            Ok(())
        });

        let mut i = 0;

        // Write the data to the tar archive

        while let Ok(mut data) = rx.recv() {
            i += 1;
            let err_msg = || {
                format!(
                    "Failed to append data for file {:?} to tar archive",
                    data.rel_path
                )
            };

            if let Some((mut cursor, mut header)) = data.cursor {
                header.set_metadata(&data.file.metadata()?);
                tar_builder
                    .append_data(&mut header, &data.rel_path, &mut cursor)
                    .with_context(err_msg)?;
            } else {
                tar_builder
                    .append_file(&data.rel_path, &mut data.file)
                    .with_context(err_msg)?;
            }

            if args.log_level >= 3 {
                println!("Added {} {:?}", i, &data.rel_path);
            }
        }

        Ok(thread)
    }

    fn join(
        src_dir: &std::path::Path,
        thread: std::thread::JoinHandle<Result<(), Error>>,
    ) -> Result<(), Error> {
        match thread.join() {
            Err(e) => {
                return Err(Error::msg(format!(
                    "Failed to join thread for processing files in directory {:?}: {:?}",
                    src_dir, e
                )));
            }
            Ok(Err(e)) => {
                return Err(Error::msg(format!(
                    "Failed to process all files in directory {:?}: {:?}",
                    src_dir, e
                )));
            }
            _ => Ok(()),
        }
    }

    fn send_tar_data(
        small_file_size: u64,
        src_dir: &std::path::Path,
        tx: &std::sync::mpsc::SyncSender<TarFileData>,
        entry: walkdir::DirEntry,
    ) -> Result<(), Error> {
        let path = entry.path();

        if path.is_dir() || path == src_dir {
            return Ok(());
        }

        let rel_path = path
            .strip_prefix(&src_dir)
            .with_context(|| format!("Failed to strip {:?} by {:?}", path, src_dir))?;

        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open file {:?} for reading", path))?;

        if small_file_size > 0 && path.metadata()?.file_size() >= small_file_size {
            // println!("Large file {}: {:?}", i, path);
            tx.send(TarFileData {
                file: file,
                rel_path: rel_path.to_path_buf(),
                cursor: None,
            })
            .with_context(|| {
                format!("Failed to send data for file {:?} to tar archive", rel_path)
            })?;
            return Ok(());
        }

        let file_data = std::fs::read(path)
            .with_context(|| format!("Failed to read file {:?} into memory", path))?;

        let cursor = std::io::Cursor::new(file_data);
        let mut header = tar::Header::new_gnu();
        header.set_metadata(&file.metadata()?);

        tx.send(TarFileData {
            file: file,
            rel_path: rel_path.to_path_buf(),
            cursor: Some((cursor, header)),
        })
        .with_context(|| format!("Failed to send data for file {:?} to tar archive", rel_path))?;

        Ok(())
    }
}

/// Creates a tarball compressed with Zstandard (zstd) algorithm and writes it to the given output.
///
/// `args` - Configuration arguments containing compression settings
/// `src_dir` - Path to the directory to be archived
/// `output` - Write implementer that receives the compressed tarball data
pub fn tar_zstd<W: Write>(args: &args::Args, src_dir: &std::path::Path, output: W) -> Result<()> {
    // ZSTD Encoder

    let err_msg = || format!("Failed to create zstd encoder for {:?}", src_dir);

    let mut level = 3;
    if let Some(l) = args.compress_level {
        level = l.min(22).max(1);
    }

    let mut zstd_encoder = zstd::stream::write::Encoder::new(output, level.into())?;
    if !args.no_long_distance_matching {
        zstd_encoder
            .long_distance_matching(true)
            .with_context(err_msg)?;
    }

    zstd_encoder
        .multithread(num_cpus::get() as u32)
        .with_context(err_msg)?;

    // Tar Builder

    let mut tar_builder = tar::Builder::new(zstd_encoder);

    // Start

    let thread = TarWriter::start(args, &src_dir, &mut tar_builder);

    // End

    let zstd_encoder = tar_builder.into_inner()?;
    zstd_encoder.finish()?;
    TarWriter::join(&src_dir, thread?)
}
