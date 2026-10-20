use std::io;

const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 0xFFFF;
const HASH_SIZE: usize = 1 << 16; // 65536

#[derive(Clone, Copy, Debug)]
pub enum CompressionLevel {
    Fast,    // -l1
    Default, // -l3
    Max,     // -l7
    Gpu,     // -g
}

// Simple hash of three bytes.
#[inline]
fn hash3(a: u8, b: u8, c: u8) -> usize {
    let h = (a as u32)
        .wrapping_mul(257)
        .wrapping_add((b as u32).wrapping_mul(263))
        .wrapping_add((c as u32).wrapping_mul(269));
    (h as usize) & (HASH_SIZE - 1)
}

/// Token format:
/// 0x00 [u16 len] [len bytes literal]
/// 0x01 [u16 offset] [u16 len]
pub fn lz_compress(input: &[u8], level: CompressionLevel) -> Vec<u8> {
    let n = input.len();
    if n == 0 {
        return Vec::new();
    }

    // Compression parameters by level.
    let (window_size, level_max_match_len): (usize, usize) = match level {
        CompressionLevel::Fast => (2 * 1024, 32), // Small window and short matches.
        CompressionLevel::Default => (4 * 1024, 256), // Balanced setting.
        CompressionLevel::Max | CompressionLevel::Gpu => (16 * 1024, MAX_MATCH),
    };

    let mut out = Vec::with_capacity(n / 2);

    // Remember the most recent position for each three-byte hash.
    let mut last_pos = vec![usize::MAX; HASH_SIZE];

    let mut i = 0usize;
    let mut lit_start = 0usize;

    while i + MIN_MATCH <= n {
        let a = input[i];
        let b = input[i + 1];
        let c = input[i + 2];
        let h = hash3(a, b, c);
        let prev = last_pos[h];
        last_pos[h] = i;

        let mut best_len = 0usize;
        let mut best_off = 0usize;

        if prev != usize::MAX && prev < i {
            let dist = i - prev;
            if dist > 0 && dist <= window_size {
                let mut l = 0usize;
                let max_len = std::cmp::min(level_max_match_len, n - i);
                while l < max_len && input[prev + l] == input[i + l] {
                    l += 1;
                }
                if l >= MIN_MATCH {
                    best_len = l;
                    best_off = dist;
                }
            }
        }

        if best_len >= MIN_MATCH {
            // Flush literals before the match.
            let lit_len = i - lit_start;
            if lit_len > 0 {
                out.push(0x00);
                let len_u16 = lit_len as u16;
                out.extend_from_slice(&len_u16.to_le_bytes());
                out.extend_from_slice(&input[lit_start..i]);
            }

            // Write the match token.
            out.push(0x01);
            let offset_u16 = best_off as u16;
            let len_u16 = best_len as u16;
            out.extend_from_slice(&offset_u16.to_le_bytes());
            out.extend_from_slice(&len_u16.to_le_bytes());

            // Advance past the match and update hashes for intermediate positions.
            let end = i + best_len;
            let mut j = i + 1;
            while j + MIN_MATCH <= end {
                let a = input[j];
                let b = input[j + 1];
                let c = input[j + 2];
                let h = hash3(a, b, c);
                last_pos[h] = j;
                j += 1;
            }

            i = end;
            lit_start = i;
        } else {
            i += 1;
        }
    }

    // Write the remaining bytes as literals.
    if lit_start < n {
        out.push(0x00);
        let lit_len = n - lit_start;
        let len_u16 = lit_len as u16;
        out.extend_from_slice(&len_u16.to_le_bytes());
        out.extend_from_slice(&input[lit_start..]);
    }

    out
}

pub fn lz_decompress(input: &[u8]) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < input.len() {
        if i >= input.len() {
            break;
        }
        let tag = input[i];
        i += 1;

        match tag {
            0x00 => {
                if i + 2 > input.len() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Literal len"));
                }
                let len = u16::from_le_bytes([input[i], input[i + 1]]) as usize;
                i += 2;
                if i + len > input.len() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Literal data"));
                }
                out.extend_from_slice(&input[i..i + len]);
                i += len;
            }
            0x01 => {
                if i + 4 > input.len() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Match header"));
                }
                let offset = u16::from_le_bytes([input[i], input[i + 1]]) as usize;
                let len = u16::from_le_bytes([input[i + 2], input[i + 3]]) as usize;
                i += 4;

                if offset == 0 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Offset 0"));
                }
                if offset > out.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Match offset out of range",
                    ));
                }

                let start = out.len() - offset;
                for j in 0..len {
                    let b = *out
                        .get(start + j)
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Match OOB"))?;
                    out.push(b);
                }
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unknown token tag {}", tag),
                ));
            }
        }
    }

    Ok(out)
}
