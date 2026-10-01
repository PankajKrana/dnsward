//! Safe file writing helpers.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Atomically replaces `path` with `data`: writes a temporary file in the
/// same directory, fsyncs it, renames it over the target and fsyncs the
/// directory. Readers observe either the old or the new content, never a
/// partial write. `mode` sets the Unix permission bits of the new file.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_string_lossy();
    let tmp = dir.join(format!(".{name}.tmp.{}", std::process::id()));
    let result = (|| {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        opts.mode(mode);
        #[cfg(not(unix))]
        let _ = mode;
        let mut file = opts.open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Creates a directory (and parents) and sets its permission bits.
pub fn ensure_dir(path: &Path, mode: u32) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_replace() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.txt");
        write_atomic(&p, b"one", 0o644).unwrap();
        write_atomic(&p, b"two", 0o600).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // No temporary files left behind.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
