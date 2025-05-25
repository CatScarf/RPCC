use anyhow::{Context, Error, Result};
use rayon::prelude::*;
use std::io::{Read, Write};

use crate::utils;

/// Creates a zip file with dflate algorithm and writes it to the given output.
pub fn zip<W: std::io::Write + std::io::Seek + ?Sized>(
    src_dir: &std::path::Path,
    output: &mut W,
    log_level: u8,
) -> Result<(), Error> {
    let mut total_zip_writer = zip::ZipWriter::new(output);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let (tx, rx) = std::sync::mpsc::sync_channel(100);
    let src_dir_buf = src_dir.to_path_buf();

    let thread = std::thread::spawn(move || -> Result<(), Error> {
        let result = walkdir::WalkDir::new(&src_dir_buf)
            .into_iter()
            .par_bridge()
            .map(|entry| -> Result<(), Error> {
                let entry = entry.with_context(|| {
                    format!("Failed to read entry in directory {:?}", src_dir_buf)
                })?;
                if entry.file_type().is_dir() {
                    return Ok(());
                }

                let path = entry.path();
                let relpath_str = path
                    .strip_prefix(&src_dir_buf)
                    .with_context(|| format!("Failed to strip prefix from path {:?}", path))?
                    .to_string_lossy()
                    .to_string();

                let raw_size = path
                    .metadata()
                    .with_context(|| format!("Failed to get metadata for path {:?}", path))?
                    .len();

                let mut buff = std::io::Cursor::new(Vec::new());
                {
                    let mut zip_writer = zip::ZipWriter::new(&mut buff);
                    zip_writer.start_file(&relpath_str, options.clone())?;
                    let data = std::fs::read(path)?;
                    zip_writer.write_all(&data)?;
                    zip_writer.finish()?;
                }

                let zip_archive = zip::ZipArchive::new(buff)?;

                tx.send((relpath_str, zip_archive, raw_size))?;

                Ok(())
            })
            .filter(|result| !result.is_ok())
            .collect::<Vec<_>>();

        if !result.is_empty() {
            Result::Err(Error::msg(format!(
                "Failed to process all files in directory {:?}: {:?}",
                src_dir_buf, result
            )))
        } else {
            Ok(())
        }
    });

    let progress = utils::Progress::new(log_level, "C".to_string());

    while let Ok((relpath_str, mut zip_archive, raw_size)) = rx.recv() {
        let zip_file = zip_archive.by_name(&relpath_str)?;
        total_zip_writer.raw_copy_file(zip_file).with_context(|| {
            format!(
                "Failed to append data for file {:?} to zip archive",
                &relpath_str
            )
        })?;
        progress
            .tx
            .send(utils::ProgressData::Data((relpath_str, raw_size)))?;
    }
    progress.join()?;
    total_zip_writer.finish()?;

    match thread.join() {
        Ok(result) => result,
        Err(_) => Err(Error::msg("Thread panicked")),
    }
}

pub fn unzip<R: std::io::Read + std::io::Seek + ?Sized>(
    input: &mut R,
    dest_dir: &std::path::Path,
    log_level: u8,
) -> Result<(), Error> {
    let (tx, rx) = crossbeam::channel::bounded(100);

    let dest_dir_buf = dest_dir.to_path_buf().clone();
    let thread = std::thread::spawn(move || -> Result<(), Error> {
        let result = rx
            .iter()
            .par_bridge()
            .map(|(name, buf)| -> Result<(), Error> {
                let dest_path = dest_dir_buf.join(&name);
                // Handle existing file
                if dest_path.exists() {
                    std::fs::remove_file(&dest_path)?
                }

                // Handle parent directory
                if let Some(parent) = dest_path.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent)?;
                    } else if parent.is_file() {
                        std::fs::remove_file(parent)?;
                        std::fs::create_dir_all(parent)?;
                    }
                }

                std::fs::write(&dest_path, &buf)?;
                Ok(())
            })
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

    let progress = utils::Progress::new(log_level, "Dec".to_string());

    let dest_dir_buf = dest_dir.to_path_buf();
    let archive = &mut zip::ZipArchive::new(input)?;
    let num_files = archive.len();
    let result = (0..num_files)
        .into_iter()
        .map(|i| -> Result<(), Error> {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_string();
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            let len = buf.len() as u64;
            tx.send((name, buf))?;
            progress
                .tx
                .send(utils::ProgressData::Data((file.name().to_string(), len)))?;
            Ok(())
        })
        .filter(|result| !result.is_ok())
        .collect::<Vec<_>>();

    if !result.is_empty() {
        return Result::Err(Error::msg(format!(
            "Failed to unzip all files in archive: {:?} {:?}",
            dest_dir_buf, result
        )));
    }

    drop(tx);
    progress.join()?;

    match thread.join() {
        Ok(result) => result,
        Err(_) => return Err(Error::msg("Thread panicked")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests;

    #[test]
    fn test_zip() {
        let mut tester = tests::tests::Tester::new();

        zip(&tester.src_dir.path(), &mut tester.intermediate, 0).unwrap();
        tester.flush_intermediate();
        unzip(&mut tester.intermediate, &tester.dest_dir.path(), 0).unwrap();

        tester.assert();
    }
}
