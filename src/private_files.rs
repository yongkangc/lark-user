use crate::{Error, ErrorCode, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

fn unsafe_file() -> Error {
    Error::new(
        ErrorCode::UnsafeFile,
        "Private files must be owned by the current user, use 0600 permissions (directories: 0700), and not be symlinks",
    )
}

#[cfg(unix)]
fn check_metadata(metadata: &fs::Metadata, directory: bool) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let kind_ok = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !kind_ok
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(unsafe_file());
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_metadata(_: &fs::Metadata, _: bool) -> Result<()> {
    Err(Error::new(
        ErrorCode::UnsafeFile,
        "Private filesystem storage currently requires Unix permissions; Windows ACL support is not implemented",
    ))
}

pub(crate) fn directory(path: &Path) -> Result<()> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|_| Error::storage())?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| Error::storage())?;
    check_metadata(&metadata, true)
}

pub(crate) fn open(path: &Path, create: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| unsafe_file())?;
    check_metadata(&file.metadata().map_err(|_| Error::storage())?, false)?;
    Ok(file)
}

pub(crate) fn read(path: &Path, max: usize) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    open(path, false)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::storage())?;
    if bytes.len() > max {
        return Err(Error::invalid("Input exceeds the documented size limit"));
    }
    Ok(bytes)
}

pub(crate) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::invalid("Output needs a parent directory"))?;
    directory(parent)?;
    if path.symlink_metadata().is_ok() {
        open(path, false)?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| Error::storage())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::storage())?;
    }
    temporary.write_all(bytes).map_err(|_| Error::storage())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| Error::storage())?;
    temporary.persist(path).map_err(|_| Error::storage())?;
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::storage())?;
    Ok(())
}

pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::invalid("Output needs a parent directory"))?;
    directory(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| Error::storage())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| Error::storage())?;
    }
    temporary.write_all(bytes).map_err(|_| Error::storage())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| Error::storage())?;
    temporary.persist_noclobber(path).map_err(|_| {
        Error::new(
            ErrorCode::StorageError,
            "Output already exists or could not be created; no existing file was replaced",
        )
    })?;
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::storage())?;
    Ok(())
}

pub(crate) fn remove(path: &Path) -> Result<()> {
    open(path, false)?;
    fs::remove_file(path).map_err(|_| Error::storage())
}
