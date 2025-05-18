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
