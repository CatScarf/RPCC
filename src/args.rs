use clap::{Parser, ValueEnum};
use strum::Display;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Display)]
pub enum Command {
    /// Compress the input
    C,
    /// Decompress the input
    X,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CompressType {
    TARZSTD,
    ZIP,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// The command to execute
    #[arg(value_enum)]
    pub command: Command,

    /// Set type of archive
    #[arg(short = 't', long, default_value = "tarzstd")]
    pub compress_type: CompressType,

    /// Input path
    pub input: String,

    /// Output path, must be a file
    /// (defaults to input path with compression extension)
    pub output: Option<String>,

    /// Log level
    #[arg(long = "ll", default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..))]
    pub log_level: u8,

    /// Compress level
    #[arg(long = "l", short = 'l')]
    pub compress_level: Option<u8>,

    /// Disable long distance matching (only for zstd)
    #[arg(long = "noldm", default_value_t = false)]
    pub no_long_distance_matching: bool,

    /// Only size smaller than this will be read in parallel
    #[arg(long = "sfs", default_value = "10485760")]
    pub small_file_size: u64,
}
