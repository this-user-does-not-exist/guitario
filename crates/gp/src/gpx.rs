//! Guitar Pro 6 (`.gpx`): the same `score.gpif` XML as Guitar Pro 7, inside a small sector file
//! system ("BCFS"), usually compressed ("BCFZ").
//!
//! BCFZ is a bit stream read most significant bit first. Each chunk starts with a flag bit: `0` is
//! up to three literal bytes (a 2-bit count, read least significant bit first, then whole bytes);
//! `1` copies earlier output (a 4-bit field width, then a back offset and a length of that width,
//! both least significant bit first). BCFS is a list of 4 KiB sectors: a sector starting with the
//! int `2` describes a file (name at byte 4, size at 0x8C, the numbers of its data sectors from
//! 0x94, ended by 0). Written from the public format notes; nothing is copied from other
//! implementations.

use crate::{GpError, Import, MAX_GPIF};

const SECTOR: usize = 0x1000;

fn damaged(what: &str) -> GpError {
    GpError::Damaged(format!("Guitar Pro 6 container: {what}"))
}

pub(crate) fn looks_gpx(bytes: &[u8]) -> bool {
    bytes.starts_with(b"BCFZ") || bytes.starts_with(b"BCFS")
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Import, GpError> {
    let fs = if bytes.starts_with(b"BCFZ") { decompress(bytes.get(4..).unwrap_or_default())? } else { bytes.to_vec() };
    let data = fs.strip_prefix(b"BCFS").ok_or_else(|| damaged("no file system"))?;
    let xml = file(data, "score.gpif")?.ok_or(GpError::NoScore)?;
    let xml = String::from_utf8(xml).map_err(|_| GpError::Xml("score.gpif is not UTF-8".into()))?;
    crate::parse_gpif(&xml)
}

struct Bits<'a> {
    data: &'a [u8],
    /// Position in bits.
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.pos / 8)?;
        let bit = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Some(u32::from(bit))
    }

    /// `n` bits, most significant first.
    fn bits(&mut self, n: u32) -> Option<u32> {
        (0..n).try_fold(0u32, |v, _| Some((v << 1) | self.bit()?))
    }

    /// `n` bits, least significant first.
    fn bits_reversed(&mut self, n: u32) -> Option<u32> {
        (0..n).try_fold(0u32, |v, i| Some(v | (self.bit()? << i)))
    }
}

/// Inflates a BCFZ stream (the bytes after the `BCFZ` magic).
fn decompress(src: &[u8]) -> Result<Vec<u8>, GpError> {
    let len_bytes: [u8; 4] = src.get(..4).and_then(|b| b.try_into().ok()).ok_or_else(|| damaged("too short"))?;
    let expected = usize::try_from(i32::from_le_bytes(len_bytes)).map_err(|_| damaged("negative length"))?;
    let cap = usize::try_from(MAX_GPIF).unwrap_or(usize::MAX).saturating_mul(2);
    if expected > cap {
        return Err(GpError::TooLarge);
    }
    let mut bits = Bits { data: src.get(4..).unwrap_or_default(), pos: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(expected.min(1 << 24));
    let truncated = || damaged("compressed data ends early");
    while out.len() < expected {
        // Guitar Pro stops the stream a little short of the declared length; the rest is zeros.
        let Some(flag) = bits.bit() else { break };
        if flag == 1 {
            let width = bits.bits(4).ok_or_else(truncated)?;
            let offset = bits.bits_reversed(width).ok_or_else(truncated)? as usize;
            let size = bits.bits_reversed(width).ok_or_else(truncated)? as usize;
            let start = out.len().checked_sub(offset).filter(|_| offset > 0).ok_or_else(|| damaged("a back reference points before the start"))?;
            // The copy never overlaps the bytes it is writing.
            let take = size.min(offset);
            let chunk = out.get(start..start + take).ok_or_else(truncated)?.to_vec();
            out.extend(chunk);
        } else {
            let Some(count) = bits.bits_reversed(2) else { break };
            for _ in 0..count {
                let Some(b) = bits.bits(8) else { break };
                out.push(u8::try_from(b).unwrap_or(0));
            }
        }
    }
    out.resize(expected, 0);
    Ok(out)
}

fn int_at(data: &[u8], at: usize) -> Option<u32> {
    let b: [u8; 4] = data.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(b))
}

/// The contents of the file called `name` in a BCFS file system (the bytes after `BCFS`).
fn file(data: &[u8], name: &str) -> Result<Option<Vec<u8>>, GpError> {
    let mut at = SECTOR;
    while at.checked_add(SECTOR).is_some_and(|end| end <= data.len()) {
        if int_at(data, at) == Some(2) {
            let raw_name = data.get(at + 4..at + 4 + 127).unwrap_or_default();
            let entry = raw_name.split(|b| *b == 0).next().unwrap_or_default();
            let size = int_at(data, at + 0x8C).ok_or_else(|| damaged("a file entry is cut short"))? as usize;
            let mut content = Vec::new();
            let mut last = at;
            for i in 0.. {
                let Some(sector) = (at + 0x94).checked_add(4 * i).and_then(|p| int_at(data, p)) else { break };
                if sector == 0 || 0x94 + 4 * i >= SECTOR {
                    break;
                }
                let start = (sector as usize).checked_mul(SECTOR).ok_or_else(|| damaged("a sector number is out of range"))?;
                let block = data.get(start..start + SECTOR).ok_or_else(|| damaged("a sector lies past the end"))?;
                content.extend_from_slice(block);
                last = last.max(start);
                if content.len() as u64 > MAX_GPIF {
                    return Err(GpError::TooLarge);
                }
            }
            if entry == name.as_bytes() {
                content.truncate(size);
                return Ok(Some(content));
            }
            // A file's data sectors follow its entry; keep scanning after them.
            at = last;
        }
        at += SECTOR;
    }
    Ok(None)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal BCFS image holding one file, for tests.
    pub fn bcfs(name: &str, content: &[u8]) -> Vec<u8> {
        let mut fs = vec![0u8; SECTOR]; // header sector
        let mut entry = vec![0u8; SECTOR];
        entry[..4].copy_from_slice(&2u32.to_le_bytes());
        entry[4..4 + name.len()].copy_from_slice(name.as_bytes());
        entry[0x8C..0x90].copy_from_slice(&(content.len() as u32).to_le_bytes());
        let sectors = content.len().div_ceil(SECTOR);
        for i in 0..sectors {
            let n = (2 + i) as u32;
            entry[0x94 + 4 * i..0x98 + 4 * i].copy_from_slice(&n.to_le_bytes());
        }
        fs.extend(entry);
        for chunk in content.chunks(SECTOR) {
            let mut block = chunk.to_vec();
            block.resize(SECTOR, 0);
            fs.extend(block);
        }
        let mut out = b"BCFS".to_vec();
        out.extend(fs);
        out
    }

    struct BitWriter {
        out: Vec<u8>,
        n: usize,
    }

    impl BitWriter {
        fn bit(&mut self, b: u32) {
            if self.n.is_multiple_of(8) {
                self.out.push(0);
            }
            if b != 0 {
                *self.out.last_mut().unwrap() |= 1 << (7 - self.n % 8);
            }
            self.n += 1;
        }
        fn bits(&mut self, v: u32, n: u32) {
            for i in (0..n).rev() {
                self.bit((v >> i) & 1);
            }
        }
        fn bits_reversed(&mut self, v: u32, n: u32) {
            for i in 0..n {
                self.bit((v >> i) & 1);
            }
        }
    }

    /// Compresses with literals and naive back references, for tests.
    pub fn bcfz(data: &[u8]) -> Vec<u8> {
        let mut w = BitWriter { out: Vec::new(), n: 0 };
        let mut i = 0;
        while i < data.len() {
            // Look back for a match of at least 4 bytes within 255 bytes.
            let best = (1..=i.min(255))
                .map(|off| (off, (0..off.min(255)).take_while(|k| data.get(i + k) == data.get(i - off + k)).count()))
                .max_by_key(|(_, len)| *len);
            match best {
                Some((off, len)) if len >= 4 => {
                    w.bit(1);
                    w.bits(8, 4);
                    w.bits_reversed(off as u32, 8);
                    w.bits_reversed(len as u32, 8);
                    i += len;
                }
                _ => {
                    let n = (data.len() - i).min(3);
                    w.bit(0);
                    w.bits_reversed(n as u32, 2);
                    for b in &data[i..i + n] {
                        w.bits(u32::from(*b), 8);
                    }
                    i += n;
                }
            }
        }
        let mut out = b"BCFZ".to_vec();
        out.extend((data.len() as i32).to_le_bytes());
        out.extend(w.out);
        out
    }

    #[test]
    fn decompresses_what_the_test_compressor_wrote() {
        let text = b"abcabcabcabc hello hello hello world, BCFS BCFS BCFS 0123456789".repeat(40);
        let packed = bcfz(&text);
        assert!(packed.len() < text.len());
        assert_eq!(decompress(&packed[4..]).unwrap(), text);
    }

    #[test]
    fn finds_a_file_in_the_file_system() {
        let content = b"<GPIF/>".repeat(1000);
        let fs = bcfs("score.gpif", &content);
        assert_eq!(file(&fs[4..], "score.gpif").unwrap().unwrap(), content);
        assert_eq!(file(&fs[4..], "other").unwrap(), None);
    }

    #[test]
    fn damaged_containers_are_refused() {
        assert!(decompress(&[]).is_err());
        assert!(decompress(&(-5i32).to_le_bytes()).is_err());
        assert!(matches!(decompress(&(i32::MAX).to_le_bytes()), Err(GpError::TooLarge)));
        // A back reference before the start.
        let mut bad = 10i32.to_le_bytes().to_vec();
        bad.push(0b1100_0111);
        bad.push(0xff);
        assert!(decompress(&bad).is_err());
        // Every truncation and corruption of a real container fails cleanly.
        let packed = bcfz(&bcfs("score.gpif", b"<GPIF></GPIF>"));
        for n in 0..packed.len() {
            let _ = parse(&packed[..n]);
            let mut c = packed.clone();
            c[n] ^= 0xa5;
            let _ = parse(&c);
        }
        assert!(matches!(parse(b"BCFSxxxx"), Err(GpError::NoScore)));
    }
}
