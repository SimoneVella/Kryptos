//! Crash-safe file I/O: write to a temp file, fsync, keep one backup, rename.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::error::Result;

/// Creates `dir` readable only by the current user.
pub fn ensure_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    Ok(fs::read(path)?)
}

/// Atomically replaces `path` with `bytes`. The previous version is kept as `<path>.bak`
/// so a bug or a half-written file can never cost the user their whole vault.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        ensure_private_dir(dir)?;
    }
    let tmp = path.with_extension("kryptos.tmp");
    {
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if path.exists() {
        fs::copy(path, path.with_extension("kryptos.bak"))?;
    }
    fs::rename(&tmp, path)?;
    if let Some(dir) = path.parent() {
        // Persist the rename itself (no-op / unsupported on Windows).
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_with_backup() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/vault.kryptos");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(read(&p).unwrap(), b"two");
        assert_eq!(read(&p.with_extension("kryptos.bak")).unwrap(), b"one");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
