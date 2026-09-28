use crate::model::Session;
use anyhow::{Context, Result};
use std::{
    fs,
    io::ErrorKind,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub fn nexus_dir(repository: &Path) -> PathBuf {
    repository.join(".nexus")
}
fn sessions_dir(repository: &Path) -> PathBuf {
    nexus_dir(repository).join("sessions")
}

pub fn prepare(repository: &Path) -> Result<PathBuf> {
    let local = nexus_dir(repository);
    let dir = sessions_dir(repository);
    fs::create_dir_all(&dir).context("could not create Nexus session directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&local, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    let ignore = local.join(".gitignore");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(ignore)
    {
        Ok(mut file) => file.write_all(b"*\n")?,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("could not ignore local Nexus state"),
    }
    Ok(local)
}

pub fn save_session(repository: &Path, session: &Session) -> Result<()> {
    let dir = sessions_dir(repository);
    prepare(repository)?;
    let id = Uuid::parse_str(&session.id).context("invalid session id")?;
    let path = dir.join(format!("{id}.json"));
    let temporary = dir.join(format!(".{id}.{}.tmp", Uuid::new_v4()));
    let body = serde_json::to_vec_pretty(session)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .context("could not create session checkpoint")?;
    file.write_all(&body)?;
    file.sync_all()?;
    fs::rename(temporary, path).context("could not persist Nexus session")
}

pub fn load_session(repository: &Path, id: &str) -> Result<Session> {
    let id = Uuid::parse_str(id).context("session id must be a UUID")?;
    let path = sessions_dir(repository).join(format!("{id}.json"));
    let body = fs::read(path).context("session was not found in this repository")?;
    Ok(serde_json::from_slice(&body)?)
}

pub fn list_sessions(repository: &Path) -> Result<Vec<Session>> {
    let dir = sessions_dir(repository);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut sessions = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .collect::<Vec<Session>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_keeps_local_state_out_of_git() {
        let root = std::env::temp_dir().join(format!("nexus-storage-{}", Uuid::new_v4()));
        let dir = prepare(&root).unwrap();
        assert_eq!(fs::read_to_string(dir.join(".gitignore")).unwrap(), "*\n");
        assert!(dir.join("sessions").is_dir());
        fs::remove_dir_all(root).unwrap();
    }
}
