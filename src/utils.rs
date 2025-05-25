use anyhow::{Error, Result};

pub fn readable_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

    let mut num = bytes as f64;
    for i in 0..(UNITS.len() - 1) {
        if num < 1000.0 {
            return format!("{:.2}{}", num, UNITS[i]);
        }
        num /= 1024.0;
    }
    format!("{:.2}{}", num, UNITS[UNITS.len() - 1])
}

pub fn readable_elapse(seconds: f64) -> String {
    const UNITS: [&str; 5] = ["s", "m", "h", "d", "y"];
    const UNIT_SIZE: [f64; 4] = [60.0, 60.0, 24.0, 365.0];

    let mut num = seconds;
    for i in 0..(UNITS.len() - 1) {
        if num < UNIT_SIZE[i] {
            return format!("{:.2}{}", num, UNITS[i]);
        }
        num /= UNIT_SIZE[i];
    }
    format!("{:.2}{}", num, UNITS[UNITS.len() - 1])
}
pub enum ProgressData {
    Data((String, u64)),
    Print,
    Stop,
}

pub struct Progress {
    pub tx: std::sync::mpsc::Sender<ProgressData>,
    thread: std::thread::JoinHandle<std::result::Result<(), Error>>,
}

impl Progress {
    pub fn new(log_level: u8, prefix: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx_clone = tx.clone();
        let thread = std::thread::spawn(move || -> Result<(), Error> {
            let start: std::time::Instant = std::time::Instant::now();
            let mut num_files = 0;
            let mut size = 0;

            if log_level == 2 {
                std::thread::spawn(move || {
                    let mut first = true;
                    loop {
                        let interval = if first { 1 } else { 5 };
                        first = false;
                        std::thread::sleep(std::time::Duration::from_secs(interval));
                        tx_clone.send(ProgressData::Print).unwrap();
                    }
                });
            }

            while let Ok(data) = rx.recv() {
                match data {
                    ProgressData::Data((rel_path, raw_size)) => {
                        size += raw_size;
                        num_files += 1;
                        if log_level >= 3 {
                            println!(
                                "{}, {:?}",
                                Self::compress_message(&prefix, start, size, num_files),
                                rel_path
                            )
                        }
                    }
                    ProgressData::Print => {
                        println!(
                            "{}",
                            Self::compress_message(&prefix, start, size, num_files)
                        );
                    }
                    ProgressData::Stop => {
                        if log_level == 2 {
                            println!(
                                "{}",
                                Self::compress_message(&prefix, start, size, num_files)
                            );
                        }
                        break;
                    }
                }
            }

            Ok(())
        });
        Self { tx, thread }
    }

    pub fn join(self) -> Result<(), Error> {
        self.tx.send(ProgressData::Stop)?;
        self.thread.join().unwrap()?;
        Ok(())
    }

    fn compress_message(
        prefix: &String,
        start: std::time::Instant,
        size: u64,
        num_files: usize,
    ) -> String {
        let elapsed = start.elapsed();
        let speed = size as f64 / elapsed.as_secs_f64();
        format!(
            "{} {:<11}: {:>8}, {:>8}/s",
            prefix,
            num_files,
            readable_bytes(size),
            readable_bytes(speed as u64)
        )
    }
}
