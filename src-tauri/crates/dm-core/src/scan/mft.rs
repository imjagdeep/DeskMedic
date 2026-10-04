//! Fast scan: read the NTFS Master File Table straight from the volume, the
//! way WizTree does. Every file has a record there with its name, parent
//! folder and size, so a whole drive is read in one sequential pass instead
//! of opening every folder. Needs administrator rights (raw volume access).

use super::tree::RawEntry;
use super::{Progress, ScanError};
use ntfs::structured_values::{NtfsFileName, NtfsFileNamespace};
use ntfs::{Ntfs, NtfsAttributeType, NtfsFileFlags};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// NTFS record number of the root folder.
pub const ROOT_RECORD: u64 = 5;

pub fn scan(root: &Path, progress: &Progress) -> Result<(Vec<RawEntry>, u64), ScanError> {
    let volume = volume_path(root)
        .ok_or_else(|| ScanError::Ntfs(format!("{} is not a drive root", root.display())))?;
    let file = OpenOptions::new().read(true).open(&volume)?;
    let mut fs = CachedReader::new(file);
    let ntfs = Ntfs::new(&mut fs).map_err(err)?;
    let record_size = u64::from(ntfs.file_record_size());

    let mft = ntfs.file(&mut fs, 0).map_err(err)?;
    let mft_len = mft
        .data(&mut fs, "")
        .ok_or_else(|| ScanError::Ntfs("$MFT has no data".into()))?
        .map_err(err)?
        .to_attribute()
        .map_err(err)?
        .value_length();
    let records = mft_len / record_size;

    let mut entries = Vec::with_capacity(records as usize / 2);
    let mut skipped = 0u64;
    for n in 0..records {
        if n % 4096 == 0 && progress.cancelled() {
            return Err(ScanError::Cancelled);
        }
        let Ok(file) = ntfs.file(&mut fs, n) else {
            skipped += 1;
            continue;
        };
        if !file.flags().contains(NtfsFileFlags::IN_USE) {
            continue;
        }
        match read_record(&file, &mut fs) {
            Ok(Some((name, parent, size))) => {
                let is_dir = file.is_directory();
                if !is_dir {
                    progress.add(size);
                }
                entries.push(RawEntry {
                    key: n,
                    parent,
                    name,
                    size: if is_dir { 0 } else { size },
                    is_dir,
                });
            }
            // No usable name: an extension record of another file.
            Ok(None) => {}
            Err(_) => skipped += 1,
        }
    }
    Ok((entries, skipped))
}

/// One pass over a record's attributes: the best long name with its parent,
/// and the size of the unnamed data stream.
fn read_record(
    file: &ntfs::NtfsFile<'_>,
    fs: &mut CachedReader,
) -> ntfs::Result<Option<(String, u64, u64)>> {
    let mut best: Option<(u8, String, u64)> = None;
    let mut size = 0u64;
    let mut have_data = false;
    let mut attrs = file.attributes();
    while let Some(item) = attrs.next(fs) {
        let item = item?;
        let attr = item.to_attribute()?;
        match attr.ty()? {
            NtfsAttributeType::FileName => {
                let fname = attr.structured_value::<_, NtfsFileName>(fs)?;
                // Prefer the long Windows name; DOS 8.3 aliases are skipped.
                let rank = match fname.namespace() {
                    NtfsFileNamespace::Win32 | NtfsFileNamespace::Win32AndDos => 2,
                    NtfsFileNamespace::Posix => 1,
                    NtfsFileNamespace::Dos => continue,
                };
                if best.as_ref().is_none_or(|b| rank > b.0) {
                    best = Some((
                        rank,
                        fname.name().to_string_lossy(),
                        fname.parent_directory_reference().file_record_number(),
                    ));
                }
            }
            NtfsAttributeType::Data if !have_data && attr.name()?.is_empty() => {
                size = attr.value_length();
                have_data = true;
            }
            _ => {}
        }
    }
    Ok(best.map(|(_, name, parent)| (name, parent, size)))
}

fn err(e: ntfs::NtfsError) -> ScanError {
    ScanError::Ntfs(e.to_string())
}

/// "C:\" → "\\.\C:" (the raw volume).
fn volume_path(root: &Path) -> Option<String> {
    let s = root.to_str()?;
    let b = s.as_bytes();
    let is_root = (s.len() == 2 || (s.len() == 3 && (b[2] == b'\\' || b[2] == b'/')))
        && b[0].is_ascii_alphabetic()
        && b[1] == b':';
    is_root.then(|| format!("\\\\.\\{}:", (b[0] as char).to_ascii_uppercase()))
}

/// Raw volume reads must be sector aligned. This reader fetches aligned
/// blocks and keeps the most recently used few, because the ntfs crate jumps
/// back to record 0 (the $MFT's own record) for every file it opens.
struct CachedReader {
    file: File,
    pos: u64,
    blocks: Vec<Block>,
    tick: u64,
}

struct Block {
    start: u64,
    data: Vec<u8>,
    used: u64,
}

const BLOCK: u64 = 256 * 1024;
const SLOTS: usize = 16;

impl CachedReader {
    fn new(file: File) -> Self {
        Self {
            file,
            pos: 0,
            blocks: Vec::with_capacity(SLOTS),
            tick: 0,
        }
    }

    fn block_for(&mut self, pos: u64) -> io::Result<usize> {
        self.tick += 1;
        let start = pos - pos % BLOCK;
        if let Some(i) = self.blocks.iter().position(|b| b.start == start) {
            self.blocks[i].used = self.tick;
            return Ok(i);
        }
        let mut data = vec![0u8; BLOCK as usize];
        self.file.seek(SeekFrom::Start(start))?;
        let mut filled = 0;
        while filled < data.len() {
            match self.file.read(&mut data[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        data.truncate(filled);
        let block = Block {
            start,
            data,
            used: self.tick,
        };
        if self.blocks.len() < SLOTS {
            self.blocks.push(block);
            Ok(self.blocks.len() - 1)
        } else {
            let (i, _) = self
                .blocks
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| b.used)
                .expect("SLOTS > 0");
            self.blocks[i] = block;
            Ok(i)
        }
    }
}

impl Read for CachedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let i = self.block_for(self.pos)?;
        let b = &self.blocks[i];
        let off = (self.pos - b.start) as usize;
        if off >= b.data.len() {
            return Ok(0); // end of volume
        }
        let n = buf.len().min(b.data.len() - off);
        buf[..n].copy_from_slice(&b.data[off..off + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for CachedReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.pos = match to {
            SeekFrom::Start(p) => p,
            SeekFrom::Current(d) => self
                .pos
                .checked_add_signed(d)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?,
            SeekFrom::End(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "seek from end on a raw volume",
                ))
            }
        };
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn volume_paths() {
        assert_eq!(volume_path(Path::new("C:\\")).as_deref(), Some("\\\\.\\C:"));
        assert_eq!(volume_path(Path::new("d:")).as_deref(), Some("\\\\.\\D:"));
        assert_eq!(volume_path(Path::new("C:\\Users")), None);
        assert_eq!(volume_path(Path::new("\\\\server\\share")), None);
    }

    /// The cache must return the same bytes as a plain read, across block
    /// edges and after evictions.
    #[test]
    fn cached_reader_matches_the_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..(BLOCK as usize * 3 + 1234))
            .map(|i| (i % 251) as u8)
            .collect();
        tmp.write_all(&data).unwrap();
        let mut r = CachedReader::new(File::open(tmp.path()).unwrap());
        for &pos in &[0u64, BLOCK - 10, BLOCK * 2 + 5, 7, BLOCK * 3 + 1000] {
            r.seek(SeekFrom::Start(pos)).unwrap();
            let mut buf = vec![0u8; 100];
            let mut got = 0;
            while got < buf.len() {
                let n = r.read(&mut buf[got..]).unwrap();
                if n == 0 {
                    break;
                }
                got += n;
            }
            let end = (pos as usize + 100).min(data.len());
            assert_eq!(&buf[..got], &data[pos as usize..end], "at {pos}");
        }
    }

    /// Needs admin: compares the fast scan of C: with what Windows reports.
    /// Run with `cargo test -p dm-core -- --ignored mft_scan_of_c` elevated.
    #[test]
    #[ignore]
    fn mft_scan_of_c() {
        let p = Progress::default();
        let start = std::time::Instant::now();
        let (entries, skipped) = scan(Path::new("C:\\"), &p).unwrap();
        let t = super::super::Tree::build(
            "C:\\".into(),
            ROOT_RECORD,
            entries,
            super::super::ScanMethod::Mft,
            0,
            skipped,
        );
        let used = crate::drives::list()
            .into_iter()
            .find(|d| d.is_system)
            .map(|d| d.used_bytes())
            .unwrap();
        eprintln!(
            "MFT: {} files, {} folders, {:.1} GB in {:?}, skipped {}; Windows says {:.1} GB used",
            t.summary.files,
            t.summary.folders,
            t.summary.bytes as f64 / 1e9,
            start.elapsed(),
            skipped,
            used as f64 / 1e9
        );
        assert!(t.summary.files > 10_000);
    }
}
