//! Small cross-platform primitives for crash-safe local transactions.
//!
//! Callers remain responsible for validating path ownership and transaction
//! state. This adapter only provides the OS-specific durability boundary.

use std::path::Path;

/// Durably rename one entry within the same canonical parent directory.
///
/// Configuration transactions use this stricter primitive so a replaced path
/// cannot escape the directory whose ownership the caller already validated.
pub(crate) fn durable_sibling_rename(source: &Path, destination: &Path) -> std::io::Result<()> {
    let source_parent = canonical_parent(source, "source")?;
    let destination_parent = canonical_parent(destination, "destination")?;
    if source_parent != destination_parent {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "durable sibling rename crosses directories: {} -> {}",
                source.display(),
                destination.display()
            ),
        ));
    }

    let source_name = source.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "source has no file name")
    })?;
    let destination_name = destination.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination has no file name",
        )
    })?;
    durable_rename(
        &source_parent.join(source_name),
        &destination_parent.join(destination_name),
    )
}

fn canonical_parent(path: &Path, role: &str) -> std::io::Result<std::path::PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{role} has no parent directory"),
        )
    })?;
    std::fs::canonicalize(parent)
}

#[cfg(windows)]
pub(crate) fn durable_rename(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    fn null_terminated(path: &Path) -> std::io::Result<Vec<u16>> {
        let mut encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if encoded.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("path contains an embedded NUL: {}", path.display()),
            ));
        }
        encoded.push(0);
        Ok(encoded)
    }

    // Canonicalizing the existing source and destination parent gives Win32
    // extended-length paths without requiring the destination itself to exist.
    let source = std::fs::canonicalize(source)?;
    let destination_parent = destination.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination has no parent directory",
        )
    })?;
    let destination_parent = std::fs::canonicalize(destination_parent)?;
    let destination_name = destination.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination has no file name",
        )
    })?;
    let destination = destination_parent.join(destination_name);
    let source = null_terminated(&source)?;
    let destination = null_terminated(&destination)?;

    // SAFETY: both buffers are NUL-terminated and live for the duration of the
    // call. MOVEFILE_COPY_ALLOWED is deliberately absent, so this stays an
    // atomic same-volume rename instead of degrading into copy-and-delete.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

#[cfg(not(windows))]
pub(crate) fn durable_rename(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(windows)]
pub(crate) fn sync_directory(path: &Path) -> std::io::Result<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    type Handle = *mut c_void;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: Handle,
        ) -> Handle;
        fn FlushFileBuffers(file: Handle) -> i32;
        fn CloseHandle(object: Handle) -> i32;
    }

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let flushed = unsafe { FlushFileBuffers(handle) };
    let flush_error = (flushed == 0).then(std::io::Error::last_os_error);
    unsafe {
        CloseHandle(handle);
    }
    match flush_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let suffix = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("d2rhub-durable-fs-{}-{suffix}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create durable-fs test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn sibling_rename_moves_an_entry_inside_its_verified_directory() {
        let root = TestDirectory::new();
        let source = root.0.join("source.json");
        let destination = root.0.join("destination.json");
        std::fs::write(&source, b"committed").expect("write source");

        durable_sibling_rename(&source, &destination).expect("rename siblings");

        assert!(!source.exists());
        assert_eq!(
            std::fs::read(&destination).expect("read destination"),
            b"committed"
        );
    }

    #[test]
    fn sibling_rename_rejects_a_cross_directory_destination() {
        let root = TestDirectory::new();
        let source_parent = root.0.join("source");
        let destination_parent = root.0.join("destination");
        std::fs::create_dir_all(&source_parent).expect("create source parent");
        std::fs::create_dir_all(&destination_parent).expect("create destination parent");
        let source = source_parent.join("config.json");
        let destination = destination_parent.join("config.json");
        std::fs::write(&source, b"owned").expect("write source");

        let error = durable_sibling_rename(&source, &destination)
            .expect_err("cross-directory rename must fail closed");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(source.exists());
        assert!(!destination.exists());
    }
}
