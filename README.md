# RPCC

Rust Parallel Compressor for CLI, optimized for large volumes of small files.

## Download

You can download the latest release from the [Releases](https://github.com/CatScarf/rpcc/releases) or build it from source:

```bash
# Build
git clone git@github.com:CatScarf/rpcc.git
cargo build --release
```

## Usage

```bash
# Compress ./test to ./test.tar.zst
rpcc c ./test
# Decompress ./test.tar.zst to ./
rpcc x ./test.tar.zst
```

## Options

```text
Usage: rpcc.exe [OPTIONS] <COMMAND> <INPUT> [OUTPUT]

Arguments:
  <COMMAND>
          The command to execute

          Possible values:
          - c: Compress the input
          - x: Decompress the input

  <INPUT>
          Input path

  [OUTPUT]
          Output path, must be a file (defaults to input path with compression extension)

Options:
  -t, --compress-type <COMPRESS_TYPE>
          Set type of archive

          [default: tarzstd]

          Possible values:
          - tarzstd: Archive to TAR and compress with ZSTD / Decompress with ZSTD and extract from TAR

      --ll <LOG_LEVEL>
          Log level

          [default: 1]

  -l, --l...
          Compress level

      --noldm
          Disable long distance matching (only for zstd)

      --sfs <SMALL_FILE_SIZE>
          Only size smaller than this will be read in parallel

          [default: 10485760]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
