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

    fn prepare_compress_test_data() -> (tempfile::TempDir, tempfile::TempDir) {
        // Create temporary directory

        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();

        std::fs::create_dir_all(&src_dir).unwrap();

        // Test files

        let test_small = src_dir.path().join("test_small.txt");
        std::fs::write(&test_small, "This is a small test file.").unwrap();

        let test_small2 = src_dir.path().join("dir/test_small.txt");
        std::fs::create_dir_all(test_small2.parent().unwrap()).unwrap();
        std::fs::write(&test_small2, "This is a small test file in a subdirectory.").unwrap();

        let long_name: String = (1..=50)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("_");
        let test_long = src_dir.path().join(format!("{}.txt", long_name));
        std::fs::write(&test_long, "This is a log file with a long name.").unwrap();

        let test_big = src_dir.path().join("test_big.txt");
        let size_mb = 30;
        let mut data = String::with_capacity(size_mb * 1024 * 1024);
        while data.len() < size_mb * 1024 * 1024 {
            data.push_str("This is a big test file.\n");
        }
        std::fs::write(&test_big, &data[..size_mb * 1024 * 1024]).unwrap();

        (src_dir, dest_dir)
    }

    pub struct Tester {
        pub src_dir: tempfile::TempDir,
        pub dest_dir: tempfile::TempDir,
        pub before_hash: HashMap<String, String>,
        pub intermediate: std::io::Cursor<Vec<u8>>,
    }

    impl Tester {
        pub fn new() -> Self {
            let (src_dir, dest_dir) = prepare_compress_test_data();
            let before_hash = calculate_hash(&src_dir.path()).unwrap();

            let buf = Vec::new();
            let intermediate = std::io::Cursor::new(buf);

            Self {
                src_dir,
                dest_dir,
                before_hash,
                intermediate,
            }
        }

        pub fn assert(&self) {
            let after_hash = calculate_hash(&self.dest_dir.path()).unwrap();
            assert!(self.src_dir.path().exists());
            assert!(self.dest_dir.path().exists());
            assert_eq!(self.before_hash, after_hash);
        }

        pub fn flush_intermediate(&mut self) {
            self.intermediate.set_position(0);
        }
    }
}
