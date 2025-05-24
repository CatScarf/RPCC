#[cfg(test)]
pub mod tests {
    use anyhow::{Error, Result};
    use std::collections::HashMap;
    use std::path::Path;

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

    fn prepare_compress_test_data() -> (std::path::PathBuf, std::path::PathBuf, tempfile::TempDir) {
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

        (src_dir, dest_dir, temp_dir)
    }

    pub fn test_compression_roundtrip<C, D>(compress_fn: C, decompress_fn: D)
    where
        C: Fn(&std::path::Path, &mut dyn std::io::Write) -> Result<()>,
        D: Fn(&mut dyn std::io::Read, &std::path::Path) -> Result<()>,
    {
        // Prepare test data
        let (src_dir, dest_dir, temp_dir) = prepare_compress_test_data();

        // Calculate md5 to hashmap
        let before_hash = calculate_hash(&src_dir).unwrap();

        // Compress the directory
        let mut buf: Vec<u8> = Vec::new();
        compress_fn(&src_dir, &mut buf).unwrap();

        // Decompress the tarball
        let mut reader = std::io::Cursor::new(buf);
        decompress_fn(&mut reader, &dest_dir).unwrap();

        // Calculate md5 to hashmap
        let after_hash = calculate_hash(&dest_dir).unwrap();

        assert!(temp_dir.path().exists());
        assert_eq!(before_hash, after_hash);
    }
}
