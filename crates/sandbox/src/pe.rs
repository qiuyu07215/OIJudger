//! Patch the stack reserve size in a PE executable.
//!
//! On Windows the main thread's stack size comes from the PE header
//! (`SizeOfStackReserve`, 2 MiB by default for MinGW), so deep recursion that
//! is fine on Linux judges crashes with STATUS_STACK_OVERFLOW. We rewrite the
//! header of the judged copy so the stack can grow up to the memory limit.
//! Reserved-but-untouched stack pages are not committed, so this does not
//! count against the memory limit until actually used.

use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

const OFFSET_STACK_RESERVE: u64 = 72; // same offset for PE32 and PE32+

pub fn set_stack_reserve(exe: &Path, bytes: u64) -> io::Result<()> {
    let mut f = OpenOptions::new().read(true).write(true).open(exe)?;
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, format!("not a PE file: {m}"));

    let mut mz = [0u8; 64];
    f.read_exact(&mut mz)?;
    if &mz[0..2] != b"MZ" {
        return Err(bad("missing MZ"));
    }
    let e_lfanew = u32::from_le_bytes(mz[0x3c..0x40].try_into().unwrap()) as u64;

    f.seek(SeekFrom::Start(e_lfanew))?;
    let mut sig = [0u8; 4];
    f.read_exact(&mut sig)?;
    if &sig != b"PE\0\0" {
        return Err(bad("missing PE signature"));
    }
    let opt = e_lfanew + 4 + 20; // signature + COFF file header
    f.seek(SeekFrom::Start(opt))?;
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic)?;
    f.seek(SeekFrom::Start(opt + OFFSET_STACK_RESERVE))?;
    match u16::from_le_bytes(magic) {
        0x20b => f.write_all(&bytes.to_le_bytes())?,
        0x10b => {
            let v = bytes.min(u32::MAX as u64 & !0xffff) as u32;
            f.write_all(&v.to_le_bytes())?
        }
        _ => return Err(bad("unknown optional header magic")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pe() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.exe");
        std::fs::write(&p, vec![0u8; 256]).unwrap();
        assert!(set_stack_reserve(&p, 1 << 20).is_err());
    }

    #[test]
    fn patches_pe32_plus() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.exe");
        let mut img = vec![0u8; 512];
        img[0..2].copy_from_slice(b"MZ");
        img[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        img[0x80..0x84].copy_from_slice(b"PE\0\0");
        let opt = 0x80 + 24;
        img[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        std::fs::write(&p, &img).unwrap();
        set_stack_reserve(&p, 0x2000_0000).unwrap();
        let out = std::fs::read(&p).unwrap();
        let v = u64::from_le_bytes(out[opt + 72..opt + 80].try_into().unwrap());
        assert_eq!(v, 0x2000_0000);
    }
}
