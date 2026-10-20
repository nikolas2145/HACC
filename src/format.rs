use std::io::{self, Read, Write};
use std::path::PathBuf;

pub const MAGIC: &[u8; 4] = b"HACC";
pub const VERSION: u8 = 2;

#[derive(Debug, Clone, Copy)]
pub struct ArchiveHeader {
    pub version: u8,
    pub flags: u8,
}

impl ArchiveHeader {
    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        w.write_all(MAGIC)?;
        w.write_all(&[self.version])?;
        w.write_all(&[self.flags])?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Bad magic"));
        }

        let mut buf = [0u8; 2];
        r.read_exact(&mut buf)?;
        if buf[0] == 0 || buf[0] > VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsupported archive version",
            ));
        }
        Ok(Self {
            version: buf[0],
            flags: buf[1],
        })
    }
}

/// Simple u64 varint (similar to LEB128).
pub fn write_varint<W: Write>(mut w: W, mut v: u64) -> io::Result<()> {
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        w.write_all(&[byte])?;
        if v == 0 {
            break;
        }
    }
    Ok(())
}

pub fn read_varint<R: Read>(mut r: R) -> io::Result<u64> {
    let mut result = 0u64;
    let mut shift = 0u32;
    loop {
        let mut byte = [0u8; 1];
        r.read_exact(&mut byte)?;
        let b = byte[0];
        result |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            break;
        }
        shift += 7;
        if shift > 63 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Varint too long",
            ));
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecId {
    /// CPU LZ-like codec.
    LzSimple = 0,
    /// Uncompressed block.
    Stored = 1,
    /// Block compressed and decompressed by the GPU codec.
    Gpu = 2,
}

impl CodecId {
    pub fn from_u8(v: u8) -> io::Result<Self> {
        match v {
            0 => Ok(CodecId::LzSimple),
            1 => Ok(CodecId::Stored),
            2 => Ok(CodecId::Gpu),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported codec_id {}", v),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FilterId {
    None = 0,
}

impl FilterId {
    pub fn from_u8(v: u8) -> io::Result<Self> {
        match v {
            0 => Ok(FilterId::None),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported filter_id {}", v),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlockEntry {
    pub offset: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub codec_id: CodecId,
    pub filter_id: FilterId,
    /// CRC32 of the uncompressed block.
    pub checksum: u32,
}

impl BlockEntry {
    pub fn write_to<W: Write>(&self, w: &mut W) -> io::Result<()> {
        // Reborrow the writer to keep it available for subsequent fields.
        write_varint(&mut *w, self.offset)?;
        write_varint(&mut *w, self.compressed_size)?;
        write_varint(&mut *w, self.uncompressed_size)?;
        w.write_all(&[self.codec_id as u8, self.filter_id as u8])?;
        w.write_all(&self.checksum.to_le_bytes())?;
        Ok(())
    }

    pub fn read_from<R: Read>(r: &mut R) -> io::Result<Self> {
        // Reborrow the reader for each varint.
        let offset = read_varint(&mut *r)?;
        let compressed_size = read_varint(&mut *r)?;
        let uncompressed_size = read_varint(&mut *r)?;

        let mut buf = [0u8; 2];
        r.read_exact(&mut buf)?;
        let codec_id = CodecId::from_u8(buf[0])?;
        let filter_id = FilterId::from_u8(buf[1])?;

        let mut cbuf = [0u8; 4];
        r.read_exact(&mut cbuf)?;
        let checksum = u32::from_le_bytes(cbuf);

        Ok(BlockEntry {
            offset,
            compressed_size,
            uncompressed_size,
            codec_id,
            filter_id,
            checksum,
        })
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub uncompressed_size: u64,
    pub first_block_index: u64,
    pub num_blocks: u64,
}

impl FileEntry {
    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        w.write_all(&[u8::from(self.is_dir)])?;
        let path_str = self.path.to_string_lossy();
        write_varint(&mut w, path_str.len() as u64)?;
        w.write_all(path_str.as_bytes())?;
        write_varint(&mut w, self.uncompressed_size)?;
        write_varint(&mut w, self.first_block_index)?;
        write_varint(&mut w, self.num_blocks)?;
        Ok(())
    }

    pub fn read_from<R: Read>(mut r: R, version: u8) -> io::Result<Self> {
        let is_dir = if version >= 2 {
            let mut kind = [0u8; 1];
            r.read_exact(&mut kind)?;
            match kind[0] {
                0 => false,
                1 => true,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid entry type",
                    ))
                }
            }
        } else {
            false
        };
        let path_len = read_varint(&mut r)? as usize;
        let mut buf = vec![0u8; path_len];
        r.read_exact(&mut buf)?;
        let path = String::from_utf8(buf)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 path"))?;
        let uncompressed_size = read_varint(&mut r)?;
        let first_block_index = read_varint(&mut r)?;
        let num_blocks = read_varint(&mut r)?;
        Ok(Self {
            path: PathBuf::from(path),
            is_dir,
            uncompressed_size,
            first_block_index,
            num_blocks,
        })
    }
}
