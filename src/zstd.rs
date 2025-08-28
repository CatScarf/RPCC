use std::io::{Read, Write};

use anyhow::{Context, Error, Result};
use num_cpus;
use rayon::prelude::*;

use crate::utils;

struct TarFileData {
    rel_path: std::path::PathBuf,
    file: std::fs::File,
    cursor: Option<(std::io::Cursor<Vec<u8>>, tar::Header)>,
}

struct TarWriter;

impl TarWriter {
    fn start(
        src_dir: &std::path::Path,
        tar_builder: &mut tar::Builder<impl std::io::Write>,
        small_file_size: u64,
        log_level: u8,
    ) -> Result<std::thread::JoinHandle<Result<(), Error>>, Error> {
        let progress = utils::Progress::new(log_level, "+".to_string());

        let (tx, rx) = std::sync::mpsc::sync_channel(100);
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

        // Write the data to the tar archive

        while let Ok(mut data) = rx.recv() {
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

            progress.tx.send(utils::ProgressData::Data((
                data.rel_path.to_string_lossy().to_string(),
                data.file.metadata()?.len(),
            )))?;
        }

        progress.join()?;

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
        if path.is_dir() {
            return Ok(());
        }

        let relpath = path
            .strip_prefix(&src_dir)
            .with_context(|| format!("Failed to strip {:?} by {:?}", path, src_dir))?;

        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open file {:?} for reading", path))?;

        if small_file_size > 0 && path.metadata()?.len() >= small_file_size {
            // println!("Large file {}: {:?}", i, path);
            tx.send(TarFileData {
                file: file,
                rel_path: relpath.to_path_buf(),
                cursor: None,
            })
            .with_context(|| {
                format!("Failed to send data for file {:?} to tar archive", relpath)
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
            rel_path: relpath.to_path_buf(),
            cursor: Some((cursor, header)),
        })
        .with_context(|| format!("Failed to send data for file {:?} to tar archive", relpath))?;

        Ok(())
    }
}

/// Creates a tarball compressed with Zstandard (zstd) algorithm and writes it to the given output.
pub fn tar_zstd<W: std::io::Write + ?Sized>(
    src_dir: &std::path::Path,
    output: &mut W,
    compress_level: u8,
    no_long_distance_matching: bool,
    small_file_size: u64,
    log_level: u8,
) -> Result<()> {
    // ZSTD Encoder

    let err_msg = || format!("Failed to create zstd encoder for {:?}", src_dir);

    let level = compress_level.min(22).max(1);

    let mut zstd_encoder = zstd::stream::write::Encoder::new(output, level.into())?;
    if !no_long_distance_matching {
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

    let thread = TarWriter::start(&src_dir, &mut tar_builder, small_file_size, log_level);

    // End

    let zstd_encoder = tar_builder.into_inner()?;
    zstd_encoder.finish()?;
    TarWriter::join(&src_dir, thread?)
}

/// Extracts a tarball compressed with Zstandard (zstd) algorithm from the given input.
pub fn untar_zstd<R: std::io::Read + ?Sized>(
    input: &mut R,
    dest_dir: &std::path::Path,
    log_level: u8,
) -> Result<(), Error> {
    // Create destination directory if it doesn't exist

    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create destination directory {:?}", dest_dir))?;

    // ZSTD Decoder

    let err_msg = || format!("Failed to create zstd decoder for {:?}", dest_dir);

    let zstd_decoder = zstd::stream::read::Decoder::new(input).with_context(err_msg)?;

    // Tar Archive

    let mut tar_archive = tar::Archive::new(zstd_decoder);

    // Parallel writ

    let (tx, rx) = crossbeam::channel::bounded(100);
    let dest_dir_buf = dest_dir.to_path_buf();
    let thread = std::thread::spawn(move || -> Result<(), Error> {
        let result = rx
            .iter()
            .par_bridge()
            .map(
                |(path, buf, modified_time): (_, Vec<u8>, _)| -> Result<(), Error> {
                    let dest_path = dest_dir_buf.join(&path);
                    if let Some(parent) = dest_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut file = std::fs::File::create(&dest_path)?;
                    file.write_all(&buf)?;
                    let modified_time = std::time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_secs(modified_time as u64);
                    file.set_modified(modified_time)?;
                    Ok(())
                },
            )
            .filter(|result| !result.is_ok())
            .collect::<Vec<_>>();
        if !result.is_empty() {
            Result::Err(Error::msg(format!(
                "Failed to unzip all files to directory {:?}: {:?}",
                dest_dir_buf, result
            )))
        } else {
            Ok(())
        }
    });

    // Extract

    let progress = utils::Progress::new(log_level, "+".to_string());

    let entries = tar_archive.entries()?;
    for entry in entries {
        let mut entry = entry?;
        let path = dest_dir.join(entry.path()?);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let size = entry.size();
        let mut buf = Vec::new();
        let _ = entry.read_to_end(&mut buf);
        let path = entry.path()?;
        let modified_time = entry.header().mtime()?;
        tx.send((path.to_path_buf(), buf, modified_time))?;
        progress.tx.send(utils::ProgressData::Data((
            path.to_string_lossy().to_string(),
            size,
        )))?;
    }

    progress.join()?;

    drop(tx);
    if let Err(e) = thread.join() {
        Err(Error::msg(format!(
            "Failed to join thread for extracting files from tar archive: {:?}",
            e
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;

    #[test]
    fn test_tar_zstd() {
        let mut tester = tests::tests::Tester::new();

        tar_zstd(
            &tester.src_dir.path(),
            &mut tester.intermediate,
            3,
            false,
            10 * 1024 * 1024,
            0,
        )
        .unwrap();

        tester.flush_intermediate();

        untar_zstd(&mut tester.intermediate, &tester.dest_dir.path(), 0).unwrap();

        tester.assert();
    }
}
