use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use crc32fast::Hasher;

use crate::codec::{lz_compress, lz_decompress, CompressionLevel};
use crate::format::{
    read_varint, write_varint, ArchiveHeader, BlockEntry, CodecId, FileEntry, FilterId, VERSION,
};

use crate::gpu; // GPU module also provides errors in CPU-only builds.

const CPU_BLOCK_SIZE: usize = 1 << 20; // 1 MiB
const GPU_BLOCK_SIZE: usize = 8 << 20; // 8 MiB amortizes CUDA transfers

// Store CPU blocks raw when compression saves less than two percent.
const STORED_THRESHOLD_PERCENT: u64 = 98;

// =====================================================================================
//  ARCHIVE WRITER
// =====================================================================================

#[derive(Debug)]
struct PendingFile {
    src_path: PathBuf,
    archive_path: PathBuf,
    is_dir: bool,
}

#[derive(Debug)]
pub struct ArchiveWriter {
    dest_path: PathBuf,
    level: CompressionLevel,
    pending_files: Vec<PendingFile>,
    names: HashSet<String>,
}

fn validate_archive_path(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsafe archive path",
        ));
    }
    Ok(())
}

impl ArchiveWriter {
    pub fn create(path: &Path, level: CompressionLevel) -> io::Result<Self> {
        if matches!(level, CompressionLevel::Gpu) && !cfg!(feature = "gpu") {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "GPU mode requires a build with --features gpu",
            ));
        }
        Ok(Self {
            dest_path: path.to_path_buf(),
            level,
            pending_files: Vec::new(),
            names: HashSet::new(),
        })
    }

    pub fn add_file(&mut self, src_path: &Path, archive_path: PathBuf) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(src_path)?;
        if !meta.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a file", src_path.display()),
            ));
        }

        self.register(src_path, archive_path, false)
    }

    pub fn add_path(&mut self, src_path: &Path, archive_path: PathBuf) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(src_path)?;
        if meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Symbolic links are unsupported",
            ));
        }
        if meta.is_file() {
            return self.add_file(src_path, archive_path);
        }
        if !meta.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsupported input type",
            ));
        }
        self.register(src_path, archive_path.clone(), true)?;
        let mut children: Vec<_> = std::fs::read_dir(src_path)?.collect::<io::Result<_>>()?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            self.add_path(&child.path(), archive_path.join(child.file_name()))?;
        }
        Ok(())
    }

    fn register(&mut self, src_path: &Path, archive_path: PathBuf, is_dir: bool) -> io::Result<()> {
        validate_archive_path(&archive_path)?;
        if !is_dir
            && self.dest_path.exists()
            && src_path.canonicalize()? == self.dest_path.canonicalize()?
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Archive cannot include its own output file",
            ));
        }
        let key = archive_path
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        if !self.names.insert(key) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Duplicate archive path",
            ));
        }
        self.pending_files.push(PendingFile {
            src_path: src_path.to_path_buf(),
            archive_path,
            is_dir,
        });
        Ok(())
    }

    /// Stream file data in blocks of up to 1 MiB on CPU or 8 MiB on GPU.
    pub fn finish(self) -> io::Result<()> {
        let mut dest = File::create(&self.dest_path)?;
        let block_size = if matches!(self.level, CompressionLevel::Gpu) {
            GPU_BLOCK_SIZE
        } else {
            CPU_BLOCK_SIZE
        };

        // 1) Calculate file entry metadata.
        let mut file_entries = Vec::with_capacity(self.pending_files.len());
        let mut total_blocks: u64 = 0;

        for pf in &self.pending_files {
            let size = if pf.is_dir {
                0
            } else {
                std::fs::metadata(&pf.src_path)?.len()
            };

            let num_blocks = if size == 0 {
                0
            } else {
                (size + block_size as u64 - 1) / block_size as u64
            };

            let first_block_index = total_blocks;
            total_blocks += num_blocks;

            file_entries.push(FileEntry {
                path: pf.archive_path.clone(),
                is_dir: pf.is_dir,
                uncompressed_size: size,
                first_block_index,
                num_blocks,
            });
        }

        // 2) Header
        let header = ArchiveHeader {
            version: VERSION,
            flags: 0,
        };
        header.write_to(&mut dest)?;

        // 3) Entry counts.
        write_varint(&mut dest, file_entries.len() as u64)?;
        write_varint(&mut dest, total_blocks)?;

        // 4) File table
        for fe in &file_entries {
            fe.write_to(&mut dest)?;
        }

        // 5) Stream compressed blocks.
        let mut block_entries: Vec<BlockEntry> = Vec::with_capacity(total_blocks as usize);

        for (idx, pf) in self.pending_files.iter().enumerate() {
            let file_size = file_entries[idx].uncompressed_size;

            if file_size == 0 {
                continue;
            }

            let mut src = File::open(&pf.src_path)?;
            let mut remaining = file_size;

            while remaining > 0 {
                let to_read_u64 = remaining.min(block_size as u64);
                let to_read = to_read_u64 as usize;

                let mut chunk = vec![0u8; to_read];
                src.read_exact(&mut chunk)?;

                // CRC32
                let mut hasher = Hasher::new();
                hasher.update(&chunk);
                let checksum = hasher.finalize();

                // GPU mode is explicit; errors must not silently switch to CPU.
                let (compressed, codec_for_compressed): (Vec<u8>, CodecId) = match self.level {
                    CompressionLevel::Gpu => (
                        gpu::gpu_compress(&chunk)
                            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?,
                        CodecId::Gpu,
                    ),
                    _ => (lz_compress(&chunk, self.level), CodecId::LzSimple),
                };

                let compressed_size = compressed.len() as u64;

                // Stored heuristic applies only to the CPU codec. GPU mode
                // always writes GPU blocks, including incompressible data.
                let (final_data, codec_id, stored_size) = {
                    let threshold = to_read_u64 * STORED_THRESHOLD_PERCENT / 100;

                    if codec_for_compressed != CodecId::Gpu && compressed_size >= threshold {
                        (chunk, CodecId::Stored, to_read_u64)
                    } else {
                        (compressed, codec_for_compressed, compressed_size)
                    }
                };

                let offset = dest.seek(SeekFrom::Current(0))?;
                dest.write_all(&final_data)?;

                block_entries.push(BlockEntry {
                    offset,
                    compressed_size: stored_size,
                    uncompressed_size: to_read_u64,
                    codec_id,
                    filter_id: FilterId::None,
                    checksum,
                });

                remaining -= to_read_u64;
            }
        }

        // 6) Block table
        let block_table_offset = dest.seek(SeekFrom::Current(0))?;
        for be in &block_entries {
            be.write_to(&mut dest)?;
        }

        // 7) Footer (offset block table)
        dest.write_all(&block_table_offset.to_le_bytes())?;

        Ok(())
    }
}

// =====================================================================================
//  ARCHIVE READER
// =====================================================================================

#[derive(Debug)]
pub struct ArchiveReader {
    src: File,
    pub file_entries: Vec<FileEntry>,
    pub block_entries: Vec<BlockEntry>,
}

impl ArchiveReader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut src = File::open(path)?;

        let header = ArchiveHeader::read_from(&mut src)?;

        let num_files = read_varint(&mut src)?;
        let num_blocks = read_varint(&mut src)?;

        let mut file_entries = Vec::with_capacity(num_files as usize);
        for _ in 0..num_files {
            file_entries.push(FileEntry::read_from(&mut src, header.version)?);
        }

        // Footer (8 bytes).
        let file_len = src.seek(SeekFrom::End(0))?;
        if file_len < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Archive is too small",
            ));
        }

        src.seek(SeekFrom::End(-8))?;
        let mut buf = [0u8; 8];
        src.read_exact(&mut buf)?;
        let block_table_offset = u64::from_le_bytes(buf);

        // Block table
        src.seek(SeekFrom::Start(block_table_offset))?;
        let mut block_entries = Vec::with_capacity(num_blocks as usize);
        for _ in 0..num_blocks {
            block_entries.push(BlockEntry::read_from(&mut src)?);
        }

        Ok(Self {
            src,
            file_entries,
            block_entries,
        })
    }

    pub fn extract_all(&mut self, dest_dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dest_dir)?;
        for (file_index, fe) in self.file_entries.iter().enumerate() {
            validate_archive_path(&fe.path).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "Unsafe path in archive")
            })?;
            if fe.is_dir && (fe.uncompressed_size != 0 || fe.num_blocks != 0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid directory entry",
                ));
            }
            let out_path = dest_dir.join(&fe.path);
            let mut current = dest_dir.to_path_buf();
            let parts: Vec<_> = fe.path.components().collect();
            for (index, part) in parts.iter().enumerate() {
                current.push(part.as_os_str());
                if let Ok(meta) = std::fs::symlink_metadata(&current) {
                    if meta.file_type().is_symlink() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Symbolic link in extraction path",
                        ));
                    }
                }
                if index + 1 < parts.len() || fe.is_dir {
                    std::fs::create_dir(&current).or_else(|error| {
                        if error.kind() == io::ErrorKind::AlreadyExists && current.is_dir() {
                            Ok(())
                        } else {
                            Err(error)
                        }
                    })?;
                }
            }
            if fe.is_dir {
                continue;
            }
            let mut out = File::create(&out_path)?;
            let mut written = 0u64;

            for j in 0..fe.num_blocks {
                let block_index = (fe.first_block_index + j) as usize;
                let be = self.block_entries.get(block_index).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Block index out of range")
                })?;

                // Read compressed or raw block data.
                self.src.seek(SeekFrom::Start(be.offset))?;
                let mut raw_block = vec![0u8; be.compressed_size as usize];
                self.src.read_exact(&mut raw_block)?;

                // Decompress using the recorded codec.
                let decompressed = match be.codec_id {
                    CodecId::LzSimple => lz_decompress(&raw_block)?,

                    CodecId::Stored => raw_block,

                    CodecId::Gpu => {
                        match gpu::gpu_decompress(&raw_block, be.uncompressed_size as usize) {
                            Ok(buf) => buf,
                            Err(msg) => {
                                return Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    format!("GPU decompression failed: {}", msg),
                                ));
                            }
                        }
                    }
                };

                if decompressed.len() as u64 != be.uncompressed_size {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Block size mismatch",
                    ));
                }

                // CRC check
                let mut hasher = Hasher::new();
                hasher.update(&decompressed);
                if hasher.finalize() != be.checksum {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("CRC mismatch in file {}", file_index),
                    ));
                }

                out.write_all(&decompressed)?;
                written = written.checked_add(be.uncompressed_size).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "File size overflow")
                })?;
            }

            if written != fe.uncompressed_size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Size mismatch in file {}", file_index),
                ));
            }
        }

        Ok(())
    }
}
