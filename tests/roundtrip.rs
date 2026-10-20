use hacc::archive::{ArchiveReader, ArchiveWriter};
use hacc::codec::CompressionLevel;
use hacc::format::CodecId;
use std::fs;
use std::path::PathBuf;

fn roundtrip(level: CompressionLevel, expected_codec: CodecId) {
    let root = std::env::temp_dir().join(format!("hacc-test-{}-{:?}", std::process::id(), level));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.bin");
    let archive = root.join("data.hacc");
    let output = root.join("output");
    let size = if matches!(level, CompressionLevel::Gpu) {
        8_500_000
    } else {
        1_100_000
    };
    let mut data = vec![b'A'; size];
    data.extend((0..130_000).map(|i| ((i * 37) % 251) as u8));
    fs::write(&source, &data).unwrap();

    let mut writer = ArchiveWriter::create(&archive, level).unwrap();
    writer
        .add_file(&source, PathBuf::from("source.bin"))
        .unwrap();
    writer.finish().unwrap();

    let mut reader = ArchiveReader::open(&archive).unwrap();
    assert!(reader.block_entries.len() >= 2);
    assert!(reader
        .block_entries
        .iter()
        .all(|b| b.codec_id == expected_codec));
    reader.extract_all(&output).unwrap();
    assert_eq!(fs::read(output.join("source.bin")).unwrap(), data);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cpu_archive_roundtrip() {
    // The long repeated run spans two archive blocks.
    roundtrip(CompressionLevel::Default, CodecId::LzSimple);
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_archive_roundtrip() {
    roundtrip(CompressionLevel::Gpu, CodecId::Gpu);
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_compresses_repeated_text_and_preserves_random_data() {
    let line = b"ASTRO COMPANY TEST BLOCK 0\n";
    let mut input = Vec::with_capacity(8192);
    while input.len() < 4096 {
        input.extend_from_slice(line);
    }
    input.truncate(4096);
    let mut state = 0x1234_5678u32;
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        input.push(state as u8);
    }

    let compressed = hacc::gpu::gpu_compress(&input).unwrap();
    assert!(compressed.len() < input.len() * 60 / 100);
    assert_eq!(
        hacc::gpu::gpu_decompress(&compressed, input.len()).unwrap(),
        input
    );
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_reads_older_rle_blocks() {
    // HGP1: four A bytes in one run token.
    let old_block = b"HGP1\x04\0\0\0\x01\0\x02\0\x83A";
    assert_eq!(hacc::gpu::gpu_decompress(old_block, 4).unwrap(), b"AAAA");
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_reads_older_lz_blocks() {
    // HGP2: literal ABCD, then a four-byte match at distance four.
    let old_block = b"HGP2\x08\0\0\0\x01\0\x08\0\x03ABCD\x80\x04\0";
    assert_eq!(
        hacc::gpu::gpu_decompress(old_block, 8).unwrap(),
        b"ABCDABCD"
    );
}

#[cfg(feature = "gpu")]
#[test]
fn gpu_roundtrips_chunk_boundaries() {
    for size in [1, 2, 3, 127, 128, 129, 4095, 4096, 4097, 8192] {
        let input: Vec<u8> = (0..size).map(|i| (i % 29) as u8).collect();
        let packed = hacc::gpu::gpu_compress(&input).unwrap();
        assert_eq!(hacc::gpu::gpu_decompress(&packed, size).unwrap(), input);
    }
}
