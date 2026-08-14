use std::path::Path;

use anyhow::Context;

pub fn ensure_dir(path: &Path) -> Result<(), anyhow::Error> {
    if path.exists() {
        if !path.is_dir() {
            anyhow::bail!("{} exists but is not a directory", path.display());
        }
        return Ok(());
    }
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory {}", path.display()))?;
    println!("[INFO] created directory {}", path.display());
    Ok(())
}

pub fn clear_dir_contents(path: &Path) -> Result<(), anyhow::Error> {
    if !path.exists() {
        return Ok(());
    }
    for entry in
        std::fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))?
    {
        let entry = entry.context("failed to read a directory entry")?;
        let p = entry.path();
        if p.is_dir() {
            std::fs::remove_dir_all(&p)
                .with_context(|| format!("failed to remove {}", p.display()))?;
        } else {
            std::fs::remove_file(&p)
                .with_context(|| format!("failed to remove {}", p.display()))?;
        }
    }
    Ok(())
}

pub fn create_or_replace_symlink(target: &Path, link: &Path) -> Result<(), anyhow::Error> {
    match std::fs::symlink_metadata(link) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                std::fs::remove_file(link).with_context(|| {
                    format!("failed to remove existing symlink {}", link.display())
                })?;
                println!("[INFO] replaced existing symlink {}", link.display());
            } else {
                anyhow::bail!(
                    "{} already exists and is not a symlink; remove it manually before retrying",
                    link.display()
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("failed to inspect {}", link.display()));
        }
    }
    std::os::unix::fs::symlink(target, link).with_context(|| {
        format!(
            "failed to create symlink {} -> {}",
            link.display(),
            target.display()
        )
    })?;
    println!(
        "[INFO] created symlink {} -> {}",
        link.display(),
        target.display()
    );
    Ok(())
}

pub fn remove_if_symlink(path: &Path) -> Result<(), anyhow::Error> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::remove_file(path)
                .with_context(|| format!("failed to remove symlink {}", path.display()))?;
            println!("[INFO] removed symlink {}", path.display());
        }
        Ok(_) => {
            println!(
                "[WARN] {} exists and is not a symlink; leaving it alone",
                path.display()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    Ok(())
}

pub fn remove_dir_if_exists(path: &Path) -> Result<(), anyhow::Error> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            std::fs::remove_dir_all(path)
                .with_context(|| format!("failed to remove directory {}", path.display()))?;
            println!("[INFO] removed directory {}", path.display());
        }
        Ok(_) => {
            println!(
                "[WARN] {} exists and is not a directory; leaving it alone",
                path.display()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_dir_creates_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b/c");
        ensure_dir(&sub).unwrap();
        assert!(sub.is_dir());
    }

    #[test]
    fn ensure_dir_rejects_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("file");
        std::fs::write(&f, b"x").unwrap();
        assert!(ensure_dir(&f).is_err());
    }

    #[test]
    fn clear_dir_contents_empties_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("f"), b"x").unwrap();
        clear_dir_contents(dir.path()).unwrap();
        assert!(!sub.exists());
        assert!(dir.path().exists());
    }

    #[test]
    fn symlink_created_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        let link = dir.path().join("link");
        create_or_replace_symlink(&target, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
        create_or_replace_symlink(&target, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
    }

    #[test]
    fn symlink_refuses_regular_file_blocker() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let blocker = dir.path().join("link");
        std::fs::write(&target, b"x").unwrap();
        std::fs::write(&blocker, b"block").unwrap();
        assert!(create_or_replace_symlink(&target, &blocker).is_err());
    }

    #[test]
    fn remove_if_symlink_removes_symlink_only() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        remove_if_symlink(&link).unwrap();
        assert!(!link.exists());
        assert!(target.exists());
    }

    #[test]
    fn remove_if_symlink_removes_dangling_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path().join("gone"), &link).unwrap();
        remove_if_symlink(&link).unwrap();
        assert!(!link.exists());
    }

    #[test]
    fn remove_if_symlink_warns_on_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("file");
        std::fs::write(&f, b"x").unwrap();
        remove_if_symlink(&f).unwrap();
        assert!(f.exists());
    }

    #[test]
    fn remove_if_symlink_noop_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        remove_if_symlink(&dir.path().join("missing")).unwrap();
    }

    #[test]
    fn remove_dir_if_exists_removes_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("f"), b"x").unwrap();
        remove_dir_if_exists(&sub).unwrap();
        assert!(!sub.exists());
    }

    #[test]
    fn remove_dir_if_exists_noop_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        remove_dir_if_exists(&dir.path().join("missing")).unwrap();
    }
}
