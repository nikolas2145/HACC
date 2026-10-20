use std::env;
use std::path::{Path, PathBuf};

use hacc::archive::{ArchiveReader, ArchiveWriter};
use hacc::codec::CompressionLevel;

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  hacc a [-l1|-l3|-l7|-g] archive.hacc file_or_folder1 [more ...]");
    eprintln!("     -l1 = fast, -l3 = default, -l7 = max CPU, -g = GPU (CUDA)");
    eprintln!("  hacc x archive.hacc [output_dir]");
}

fn parse_level(arg: &str) -> Result<CompressionLevel, String> {
    match arg {
        "-l1" => Ok(CompressionLevel::Fast),
        "-l3" => Ok(CompressionLevel::Default),
        "-l7" => Ok(CompressionLevel::Max),
        "-g" => Ok(CompressionLevel::Gpu),
        _ => Err(format!(
            "Unknown compression level: {arg}. Use -l1, -l3, -l7, or -g"
        )),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("hacc {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        print_usage();
        return;
    }
    if args.len() < 3 {
        print_usage();
        std::process::exit(1);
    }

    let cmd = &args[1];

    match cmd.as_str() {
        "a" => {
            let mut idx = 2usize;
            let mut level = CompressionLevel::Default;

            // Optional compression level.
            if idx < args.len() && (args[idx].starts_with("-l") || args[idx] == "-g") {
                match parse_level(&args[idx]) {
                    Ok(l) => level = l,
                    Err(msg) => {
                        eprintln!("{msg}");
                        std::process::exit(1);
                    }
                }
                idx += 1;
            }

            if idx >= args.len() {
                eprintln!("Missing archive path.");
                print_usage();
                std::process::exit(1);
            }

            let archive_path = PathBuf::from(&args[idx]);
            idx += 1;

            if idx >= args.len() {
                eprintln!("Specify at least one file or folder.");
                print_usage();
                std::process::exit(1);
            }

            let mut writer: ArchiveWriter = match ArchiveWriter::create(&archive_path, level) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("Could not create archive: {e}");
                    std::process::exit(1);
                }
            };

            for file_arg in &args[idx..] {
                let p = Path::new(file_arg);
                let arc_path = match p.file_name() {
                    Some(name) => name.into(),
                    None => {
                        eprintln!("Invalid input path: {file_arg}");
                        std::process::exit(1);
                    }
                };
                if let Err(e) = writer.add_path(p, arc_path) {
                    eprintln!("Could not add {file_arg}: {e}");
                    std::process::exit(1);
                }
            }

            if let Err(e) = writer.finish() {
                eprintln!("Could not write archive: {e}");
                std::process::exit(1);
            }

            println!("Archive created: {}", archive_path.display());
        }
        "x" => {
            let archive_path = PathBuf::from(&args[2]);
            let out_dir = if args.len() >= 4 {
                PathBuf::from(&args[3])
            } else {
                PathBuf::from(".")
            };

            let mut reader: ArchiveReader = match ArchiveReader::open(&archive_path) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Could not open archive: {e}");
                    std::process::exit(1);
                }
            };

            if let Err(e) = reader.extract_all(&out_dir) {
                eprintln!("Extraction failed: {e}");
                std::process::exit(1);
            }

            println!("Extracted to {}", out_dir.display());
        }
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}
