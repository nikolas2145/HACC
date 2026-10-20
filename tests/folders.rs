use hacc::archive::ArchiveReader;
use hacc::format::{write_varint, ArchiveHeader, FileEntry};
use std::fs::{self, File};
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("hacc-{}-{}", name, std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn cli_packs_files_folders_and_empty_directories() {
    let root = fixture("folders");
    let folder = root.join("My Folder");
    fs::create_dir_all(folder.join("nested/empty")).unwrap();
    fs::write(folder.join("nested/data.txt"), b"nested content").unwrap();
    fs::write(root.join("loose.txt"), b"loose content").unwrap();
    let archive = root.join("bundle.hacc");
    let output = root.join("extracted");
    let binary = env!("CARGO_BIN_EXE_hacc");
    assert!(Command::new(binary)
        .args(["a", "-l3"])
        .arg(&archive)
        .arg(&folder)
        .arg(root.join("loose.txt"))
        .status()
        .unwrap()
        .success());
    assert!(Command::new(binary)
        .arg("x")
        .arg(&archive)
        .arg(&output)
        .status()
        .unwrap()
        .success());
    assert_eq!(
        fs::read(output.join("My Folder/nested/data.txt")).unwrap(),
        b"nested content"
    );
    assert!(output.join("My Folder/nested/empty").is_dir());
    assert_eq!(
        fs::read(output.join("loose.txt")).unwrap(),
        b"loose content"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reads_version_one_archives() {
    let root = fixture("legacy");
    let archive = root.join("old.hacc");
    let mut file = File::create(&archive).unwrap();
    ArchiveHeader {
        version: 1,
        flags: 0,
    }
    .write_to(&mut file)
    .unwrap();
    write_varint(&mut file, 1).unwrap();
    write_varint(&mut file, 0).unwrap();
    let name = b"legacy.txt";
    write_varint(&mut file, name.len() as u64).unwrap();
    file.write_all(name).unwrap();
    for _ in 0..3 {
        write_varint(&mut file, 0).unwrap();
    }
    let table_offset = file.stream_position().unwrap();
    file.write_all(&table_offset.to_le_bytes()).unwrap();
    drop(file);
    let mut reader = ArchiveReader::open(&archive).unwrap();
    let output = root.join("out");
    reader.extract_all(&output).unwrap();
    assert_eq!(fs::read(output.join("legacy.txt")).unwrap(), b"");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_parent_paths_in_archives() {
    let root = fixture("unsafe");
    let archive = root.join("unsafe.hacc");
    let mut file = File::create(&archive).unwrap();
    ArchiveHeader {
        version: 2,
        flags: 0,
    }
    .write_to(&mut file)
    .unwrap();
    write_varint(&mut file, 1).unwrap();
    write_varint(&mut file, 0).unwrap();
    FileEntry {
        path: PathBuf::from("../escaped.txt"),
        is_dir: false,
        uncompressed_size: 0,
        first_block_index: 0,
        num_blocks: 0,
    }
    .write_to(&mut file)
    .unwrap();
    let table_offset = file.stream_position().unwrap();
    file.write_all(&table_offset.to_le_bytes()).unwrap();
    drop(file);
    let mut reader = ArchiveReader::open(&archive).unwrap();
    assert!(reader.extract_all(&root.join("out")).is_err());
    assert!(!root.join("escaped.txt").exists());
    fs::remove_dir_all(root).unwrap();
}
