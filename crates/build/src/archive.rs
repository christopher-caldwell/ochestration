//! A minimal, dependency-free ZIP writer and reader.
//!
//! Entries are stored uncompressed, which keeps the format simple enough to
//! write and verify exactly.  The reader exists so an export can prove that the
//! members it selected are actually inside the archive — that its central
//! directory lists them and that their bytes hash the same — before the archive
//! is promoted from its partial name.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use orchestrate_contracts::digest_bytes;

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL: u32 = 0x0605_4b50;
const STORED: u16 = 0;
/// Fixed timestamp for reproducible archives: 2020-01-01T00:00:00 in DOS format
/// (`((2020 - 1980) << 9) | (1 << 5) | 1`).
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = 0x5021;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipMember {
    pub name: String,
    /// The SHA-256 of the member's bytes as written, so a caller can verify
    /// more than the container's own CRC.
    pub sha256: String,
    pub bytes: u64,
    pub crc32: u32,
    pub offset: u64,
}

pub struct ZipWriter {
    file: BufWriter<File>,
    members: Vec<ZipMember>,
    offset: u64,
}

impl ZipWriter {
    pub fn create(path: &Path) -> Result<Self> {
        Ok(Self {
            file: BufWriter::new(File::create(path)?),
            members: Vec::new(),
            offset: 0,
        })
    }

    /// Create an archive that must not already exist.  A caller that owns its
    /// output exclusively uses this, so an unexpected file under the same name
    /// is refused instead of being silently truncated.
    pub fn create_new(path: &Path) -> Result<Self> {
        Ok(Self {
            file: BufWriter::new(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?,
            ),
            members: Vec::new(),
            offset: 0,
        })
    }

    /// Stream one file into the archive, computing its CRC and digest while it
    /// is copied, so a large member is never held in memory.
    pub fn add_file(&mut self, name: &str, path: &Path) -> Result<&ZipMember> {
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("cannot stat export member {}", path.display()))?;
        ensure!(
            metadata.is_file(),
            "export member {} is not a regular file",
            path.display()
        );
        let mut reader = BufReader::new(
            File::open(path).with_context(|| format!("cannot read {}", path.display()))?,
        );
        let header_offset = self.offset;
        self.write_local_header(name, metadata.len())?;
        let mut crc = Crc32::new();
        let digest;
        let mut written = 0u64;
        {
            let mut hasher = DigestWriter::new(&mut crc);
            let mut buffer = vec![0u8; 64 * 1024];
            loop {
                let read = reader.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                written += read as u64;
                self.file.write_all(&buffer[..read])?;
                hasher.write_all(&buffer[..read])?;
            }
            digest = hasher.digest();
        }
        self.offset += written;
        let crc32 = crc.finish();
        self.finish_local_header(header_offset, crc32)?;
        self.members.push(ZipMember {
            name: name.into(),
            sha256: digest,
            bytes: written,
            crc32,
            offset: header_offset,
        });
        Ok(self.members.last().expect("member was just pushed"))
    }

    pub fn add_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<&ZipMember> {
        let header_offset = self.offset;
        self.write_local_header(name, bytes.len() as u64)?;
        self.file.write_all(bytes)?;
        let mut crc = Crc32::new();
        crc.update(bytes);
        self.offset += bytes.len() as u64;
        let crc32 = crc.finish();
        self.finish_local_header(header_offset, crc32)?;
        self.members.push(ZipMember {
            name: name.into(),
            sha256: digest_bytes(bytes),
            bytes: bytes.len() as u64,
            crc32,
            offset: header_offset,
        });
        Ok(self.members.last().expect("member was just pushed"))
    }

    /// Backpatch the local header's CRC field.  Strict readers (Info-ZIP
    /// `unzip`) reject an archive whose local header still carries the
    /// placeholder CRC, so the real value is written there once it is known.
    fn finish_local_header(&mut self, header_offset: u64, crc32: u32) -> Result<()> {
        self.file.flush()?;
        let file = self.file.get_mut();
        let end = file.stream_position()?;
        file.seek(SeekFrom::Start(header_offset + 14))?;
        file.write_all(&crc32.to_le_bytes())?;
        file.seek(SeekFrom::Start(end))?;
        Ok(())
    }

    fn write_local_header(&mut self, name: &str, size: u64) -> Result<()> {
        // The format written here is ZIP32: refusing an oversized member is
        // better than producing an archive whose offsets quietly wrap.
        ensure!(
            size <= u32::MAX as u64 && self.offset <= u32::MAX as u64,
            "member {name} exceeds this archive format's 4 GiB limit"
        );
        let name_bytes = name.as_bytes();
        let mut header = Vec::with_capacity(30 + name_bytes.len());
        header.extend_from_slice(&LOCAL_HEADER.to_le_bytes());
        header.extend_from_slice(&20u16.to_le_bytes()); // version needed
        header.extend_from_slice(&0u16.to_le_bytes()); // flags
        header.extend_from_slice(&STORED.to_le_bytes());
        header.extend_from_slice(&DOS_TIME.to_le_bytes());
        header.extend_from_slice(&DOS_DATE.to_le_bytes());
        // The CRC is backpatched once the member's bytes are written.
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&(size as u32).to_le_bytes());
        header.extend_from_slice(&(size as u32).to_le_bytes());
        header.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        header.extend_from_slice(&0u16.to_le_bytes()); // extra length
        header.extend_from_slice(name_bytes);
        self.file.write_all(&header)?;
        self.offset += header.len() as u64;
        Ok(())
    }

    /// Write the central directory and return the members exactly as recorded.
    pub fn finish(mut self) -> Result<Vec<ZipMember>> {
        let start = self.offset;
        ensure!(
            start <= u32::MAX as u64,
            "this archive exceeds the 4 GiB limit of the format written here"
        );
        for member in &self.members {
            let name_bytes = member.name.as_bytes();
            let mut header = Vec::with_capacity(46 + name_bytes.len());
            header.extend_from_slice(&CENTRAL_HEADER.to_le_bytes());
            header.extend_from_slice(&20u16.to_le_bytes()); // version made by
            header.extend_from_slice(&20u16.to_le_bytes()); // version needed
            header.extend_from_slice(&0u16.to_le_bytes()); // flags
            header.extend_from_slice(&STORED.to_le_bytes());
            header.extend_from_slice(&DOS_TIME.to_le_bytes());
            header.extend_from_slice(&DOS_DATE.to_le_bytes());
            header.extend_from_slice(&member.crc32.to_le_bytes());
            header.extend_from_slice(&(member.bytes as u32).to_le_bytes());
            header.extend_from_slice(&(member.bytes as u32).to_le_bytes());
            header.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes()); // extra
            header.extend_from_slice(&0u16.to_le_bytes()); // comment
            header.extend_from_slice(&0u16.to_le_bytes()); // disk
            header.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            header.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            header.extend_from_slice(&(member.offset as u32).to_le_bytes());
            header.extend_from_slice(name_bytes);
            self.file.write_all(&header)?;
        }
        let central_size = {
            let mut size = 0u64;
            for member in &self.members {
                size += 46 + member.name.len() as u64;
            }
            size
        };
        ensure!(
            central_size <= u32::MAX as u64,
            "this archive's central directory exceeds the 4 GiB limit of the format written here"
        );
        let mut end = Vec::with_capacity(22);
        end.extend_from_slice(&END_OF_CENTRAL.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes()); // disk
        end.extend_from_slice(&0u16.to_le_bytes()); // central disk
        end.extend_from_slice(&(self.members.len() as u16).to_le_bytes());
        end.extend_from_slice(&(self.members.len() as u16).to_le_bytes());
        end.extend_from_slice(&(central_size as u32).to_le_bytes());
        end.extend_from_slice(&(start as u32).to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes()); // comment length
        self.file.write_all(&end)?;
        self.file.flush()?;
        self.file.get_ref().sync_all()?;
        Ok(self.members)
    }
}

/// One member of an existing archive, as its central directory describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedMember {
    pub name: String,
    pub crc32: u32,
    pub bytes: u64,
    pub offset: u64,
}

/// Read one archive's central directory.  Nothing here trusts the archive's
/// integrity beyond what its own records say; the caller verifies hashes.
pub fn central_directory(path: &Path) -> Result<Vec<ArchivedMember>> {
    let mut file = BufReader::new(File::open(path)?);
    let size = file.get_ref().metadata()?.len();
    ensure!(
        size >= 22,
        "{} is too small to be a ZIP archive",
        path.display()
    );
    // The end record is at most 22 bytes plus a 64 KiB comment; search backwards.
    let search = size.min(22 + 65_536);
    file.seek(SeekFrom::Start(size - search))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    let mut found = None;
    // The record itself is 22 bytes; a truncated or crafted tail must be
    // rejected rather than indexed past its end.
    for index in (0..tail.len().saturating_sub(21)).rev() {
        if u32::from_le_bytes([
            tail[index],
            tail[index + 1],
            tail[index + 2],
            tail[index + 3],
        ]) == END_OF_CENTRAL
        {
            found = Some(index);
            break;
        }
    }
    let end = found.context("the archive has no end-of-central-directory record")?;
    let read_u16 = |at: usize| -> u16 { u16::from_le_bytes([tail[at], tail[at + 1]]) };
    let read_u32 = |at: usize| -> u32 {
        u32::from_le_bytes([tail[at], tail[at + 1], tail[at + 2], tail[at + 3]])
    };
    let count = read_u16(end + 10) as usize;
    let central_offset = read_u32(end + 16) as u64;
    ensure!(
        central_offset <= size,
        "the archive's central directory offset is outside the file"
    );
    file.seek(SeekFrom::Start(central_offset))?;
    let mut members = Vec::with_capacity(count);
    for _ in 0..count {
        let mut header = [0u8; 46];
        file.read_exact(&mut header)
            .context("truncated central directory")?;
        let signature = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        ensure!(
            signature == CENTRAL_HEADER,
            "corrupt central directory entry"
        );
        let crc32 = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
        let compressed =
            u32::from_le_bytes([header[20], header[21], header[22], header[23]]) as u64;
        let name_length = u16::from_le_bytes([header[28], header[29]]) as usize;
        let extra_length = u16::from_le_bytes([header[30], header[31]]) as usize;
        let comment_length = u16::from_le_bytes([header[32], header[33]]) as usize;
        let offset = u32::from_le_bytes([header[42], header[43], header[44], header[45]]) as u64;
        let mut name = vec![0u8; name_length];
        file.read_exact(&mut name)?;
        file.seek(SeekFrom::Current((extra_length + comment_length) as i64))?;
        members.push(ArchivedMember {
            name: String::from_utf8(name).context("archive member name is not UTF-8")?,
            crc32,
            bytes: compressed,
            offset,
        });
    }
    Ok(members)
}

/// Read one member back, verifying its CRC, and return its bytes and digest.
pub fn read_member(path: &Path, member: &ArchivedMember) -> Result<(Vec<u8>, String)> {
    let mut file = BufReader::new(File::open(path)?);
    file.seek(SeekFrom::Start(member.offset))?;
    let mut header = [0u8; 30];
    file.read_exact(&mut header)
        .context("truncated local file header")?;
    let signature = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    ensure!(signature == LOCAL_HEADER, "corrupt local file header");
    let name_length = u16::from_le_bytes([header[26], header[27]]) as usize;
    let extra_length = u16::from_le_bytes([header[28], header[29]]) as usize;
    file.seek(SeekFrom::Current((name_length + extra_length) as i64))?;
    let mut bytes = vec![0u8; member.bytes as usize];
    file.read_exact(&mut bytes)
        .with_context(|| format!("cannot read member {} back", member.name))?;
    let mut crc = Crc32::new();
    crc.update(&bytes);
    ensure!(
        crc.finish() == member.crc32,
        "member {} does not match its recorded CRC",
        member.name
    );
    let digest = digest_bytes(&bytes);
    Ok((bytes, digest))
}

/// The archive's members as a name-keyed map.
pub fn members_by_name(path: &Path) -> Result<BTreeMap<String, ArchivedMember>> {
    Ok(central_directory(path)?
        .into_iter()
        .map(|member| (member.name.clone(), member))
        .collect())
}

struct DigestWriter<'a> {
    crc: &'a mut Crc32,
    hasher: sha2::Sha256,
}

impl<'a> DigestWriter<'a> {
    fn new(crc: &'a mut Crc32) -> Self {
        use sha2::Digest;
        Self {
            crc,
            hasher: sha2::Sha256::new(),
        }
    }

    fn digest(self) -> String {
        use sha2::Digest;
        hex::encode(self.hasher.finalize())
    }
}

impl Write for DigestWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use sha2::Digest;
        self.crc.update(buf);
        self.hasher.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Crc32 {
    value: u32,
}

impl Crc32 {
    fn new() -> Self {
        Self { value: 0xffff_ffff }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let index = ((self.value ^ *byte as u32) & 0xff) as usize;
            self.value = (self.value >> 8) ^ TABLE[index];
        }
    }

    fn finish(self) -> u32 {
        self.value ^ 0xffff_ffff
    }
}

/// The standard reflected CRC-32 table (polynomial 0xedb88320).
static TABLE: [u32; 256] = build_table();

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ 0xedb8_8320
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}

/// Refuse an archive member name that could escape the archive's own root.
pub fn safe_member_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && name
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}
