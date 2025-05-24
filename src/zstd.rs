use anyhow::{Context, Error, Result};
use num_cpus;
use rayon::prelude::*;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

struct TarFileData {
    rel_path: PathBuf,
    file: std::fs::File,
    cursor: Option<(std::io::Cursor<Vec<u8>>, tar::Header)>,
}

struct TarWriter;

impl TarWriter {
    fn start(
        src_dir: &Path,
        tar_builder: &mut tar::Builder<impl Write>,
        small_file_size: u64,
        log_level: u8,
    ) -> Result<std::thread::JoinHandle<Result<(), Error>>, Error> {
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

            if log_level >= 3 {
                println!("Added {} {:?}", i, &data.rel_path);
            }
        }

        Ok(thread)
    }

    fn join(
        src_dir: &Path,
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
        src_dir: &Path,
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

        if small_file_size > 0 && path.metadata()?.len() >= small_file_size {
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
pub fn tar_zstd<W: Write>(
    src_dir: &Path,
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
pub fn untar_zstd<R: std::io::Read>(input: &mut R, dest_dir: &Path) -> Result<()> {
    // Create destination directory if it doesn't exist
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create destination directory {:?}", dest_dir))?;

    // ZSTD Decoder
    let err_msg = || format!("Failed to create zstd decoder for {:?}", dest_dir);

    let zstd_decoder = zstd::stream::read::Decoder::new(input).with_context(err_msg)?;

    // Tar Archive
    let mut tar_archive = tar::Archive::new(zstd_decoder);

    // Extract files
    tar_archive
        .unpack(dest_dir)
        .with_context(|| format!("Failed to extract tarball to {:?}", dest_dir))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn calculate_hash(dir: &Path) -> Result<HashMap<String, String>, Error> {
        let result = walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.file_type().is_file() {
                    let path = entry.path();
                    let mut file = std::fs::File::open(path).ok()?;
                    let mut hasher = blake3::Hasher::new();
                    std::io::copy(&mut file, &mut hasher).ok()?;
                    let hash = hasher.finalize();

                    let rel_path = path.strip_prefix(dir).ok()?;

                    Some((
                        rel_path.to_string_lossy().to_string(),
                        hash.to_hex().to_string(),
                    ))
                } else {
                    None
                }
            })
            .collect::<HashMap<_, _>>();
        Ok(result)
    }

    #[test]
    fn test_tar_zstd() {
        // Create temporary directory

        let temp_dir = tempfile::tempdir().unwrap();
        let src_dir = temp_dir.path().join("src");
        let dest_dir = temp_dir.path().join("dest");

        if src_dir.exists() {
            std::fs::remove_dir_all(&src_dir).unwrap();
        }

        if dest_dir.exists() {
            std::fs::remove_dir_all(&dest_dir).unwrap();
        }

        std::fs::create_dir_all(&src_dir).unwrap();

        // Add test files

        let test_small = src_dir.join("test_small.txt");
        let test_small2 = src_dir.join("dir/test_small.txt");
        let test_big = src_dir.join("test_big.txt");

        std::fs::write(&test_small, "This is a small test file.").unwrap();
        std::fs::create_dir_all(test_small2.parent().unwrap()).unwrap();
        std::fs::write(&test_small2, "This is a small test file in a subdirectory.").unwrap();

        let size_mb = 30;
        let mut data = String::with_capacity(size_mb * 1024 * 1024);
        while data.len() < size_mb * 1024 * 1024 {
            data.push_str("This is a big test file.\n");
        }
        std::fs::write(&test_big, &data[..size_mb * 1024 * 1024]).unwrap();

        // Calculate md5 to hashmap

        let before_hash = calculate_hash(&src_dir).unwrap();

        // Compress the directory

        let mut buf: Vec<u8> = Vec::new();
        tar_zstd(&src_dir, &mut buf, 3, false, 10 * 1024 * 1024, 0).unwrap();

        // Decompress the tarball

        let mut reader = std::io::Cursor::new(buf);
        untar_zstd(&mut reader, &dest_dir).unwrap();

        // Calculate md5 to hashmap
        let after_hash = calculate_hash(&dest_dir).unwrap();

        assert_eq!(before_hash == after_hash, true);
    }
}
