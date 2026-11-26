use crate::storage::StoreError;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::Command,
};

const PRIVATE_DATA_IGNORE_RULE: &str = "/.todoing/";

/// Keeps project-local Todoing data out of Git by default.
pub(crate) fn protect_private_project_data(root: &Path) -> Result<(), StoreError> {
    let inside_worktree = match git(root, ["rev-parse", "--is-inside-work-tree"]) {
        Ok(output) => {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(StoreError::Io(error)),
    };
    if !inside_worktree {
        return Ok(());
    }

    let tracked = git(root, ["ls-files", "--cached", "--", ".todoing"])?;
    if !tracked.status.success() {
        return Err(StoreError::InvalidTask(format!(
            "Cannot inspect Git index for private Todoing data: {}",
            String::from_utf8_lossy(&tracked.stderr).trim()
        )));
    }
    let tracked_paths = String::from_utf8_lossy(&tracked.stdout);
    if !tracked_paths.trim().is_empty() {
        return Err(StoreError::InvalidTask(format!(
            "Private Todoing data is already tracked by Git ({}). Remove it from the index with 'git rm --cached -r -- .todoing' and review the repository history before retrying.",
            tracked_paths.lines().collect::<Vec<_>>().join(", ")
        )));
    }

    ensure_ignore_rule(root)?;
    let check = git(
        root,
        [
            "check-ignore",
            "--quiet",
            "--no-index",
            ".todoing/todoing.db",
        ],
    )?;
    if !check.status.success() {
        return Err(StoreError::InvalidTask(
            "Todoing private data is not ignored by Git. Check .gitignore rules before initializing or migrating the store.".into(),
        ));
    }
    Ok(())
}

fn git<const N: usize>(root: &Path, args: [&str; N]) -> std::io::Result<std::process::Output> {
    Command::new("git").arg("-C").arg(root).args(args).output()
}

fn ensure_ignore_rule(root: &Path) -> Result<(), StoreError> {
    let path = root.join(".gitignore");
    let current = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(StoreError::Io(error)),
    };
    if current
        .lines()
        .any(|line| line.trim() == PRIVATE_DATA_IGNORE_RULE)
    {
        return Ok(());
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    if !current.is_empty() && !current.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    file.write_all(PRIVATE_DATA_IGNORE_RULE.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};
    use tempfile::tempdir;

    fn init_git(root: &Path) {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["init", "--quiet"])
            .output()
            .expect("git must be installed for Git safety tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn adds_ignore_rule_and_git_ignores_private_files() {
        let root = tempdir().unwrap();
        init_git(root.path());
        fs::write(root.path().join(".gitignore"), "/target/\n").unwrap();
        protect_private_project_data(root.path()).unwrap();
        let ignore = fs::read_to_string(root.path().join(".gitignore")).unwrap();
        assert!(ignore.lines().any(|line| line == PRIVATE_DATA_IGNORE_RULE));
        assert!(
            git(
                root.path(),
                ["check-ignore", "--quiet", ".todoing/todoing.db"]
            )
            .unwrap()
            .status
            .success()
        );
    }

    #[test]
    fn creates_gitignore_when_missing() {
        let root = tempdir().unwrap();
        init_git(root.path());
        protect_private_project_data(root.path()).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join(".gitignore")).unwrap(),
            "/.todoing/\n"
        );
    }

    #[test]
    fn refuses_when_private_files_are_already_tracked() {
        let root = tempdir().unwrap();
        init_git(root.path());
        let dir = root.path().join(".todoing");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), "private = true\n").unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(root.path())
                .args(["add", "-f", ".todoing/config.toml"])
                .output()
                .unwrap()
                .status
                .success()
        );
        let result = protect_private_project_data(root.path());
        assert!(
            matches!(result, Err(StoreError::InvalidTask(message)) if message.contains("already tracked"))
        );
        assert!(!root.path().join(".gitignore").exists());
    }

    #[test]
    fn does_nothing_outside_a_git_worktree() {
        let root = tempdir().unwrap();
        protect_private_project_data(root.path()).unwrap();
        assert!(!root.path().join(".gitignore").exists());
    }
}
