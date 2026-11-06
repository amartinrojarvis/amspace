//! Hold an advisory lock for the process lifetime, including updater relaunches.
use std::{
    fs::{File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
};

fn lock(path: &Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(path)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(file));
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(error)
    }
}

pub fn acquire() -> io::Result<Option<File>> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Missing home directory"))?;
    let directory =
        Path::new(&home).join("Library/Application Support/io.github.amartinrojarvis.amspace");
    std::fs::create_dir_all(&directory)?;
    lock(&directory.join("instance.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_instance_until_owner_exits() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("amspace-lock-{}-{unique}", std::process::id()));
        let first = lock(&path).unwrap().expect("first instance");
        assert!(
            lock(&path).unwrap().is_none(),
            "second instance must not run"
        );
        drop(first);
        let relaunched = lock(&path).unwrap().expect("relaunch after owner exits");
        drop(relaunched);
        std::fs::remove_file(path).unwrap();
    }
}
