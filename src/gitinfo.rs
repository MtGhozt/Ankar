//! Branch and changed-file count for the shell footer. The branch comes from `.git/HEAD`; the
//! count from `git status --porcelain`, run off the UI thread at most every few seconds.

use std::path::{Path, PathBuf};

/// Branch name from the contents of `.git/HEAD`; a detached HEAD shows the short hash.
pub fn parse_head(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        return Some(branch.to_owned());
    }
    (head.len() >= 7 && head.bytes().all(|b| b.is_ascii_hexdigit())).then(|| head[..7].to_owned())
}

/// Files listed by `git status --porcelain` (one per line).
pub fn count_changes(porcelain: &str) -> usize {
    porcelain.lines().filter(|l| !l.trim().is_empty()).count()
}

/// `<root>/.git`, or where a worktree's `.git` file points.
fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot = root.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let target = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
    Some(if target.is_absolute() { target } else { root.join(target) })
}

pub fn branch(root: &Path) -> Option<String> {
    parse_head(&std::fs::read_to_string(git_dir(root)?.join("HEAD")).ok()?)
}

/// `git status --porcelain` in `root`; `None` when git is missing or fails. No console window.
pub fn status_porcelain(root: &Path) -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(root).args(["status", "--porcelain"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_names_the_branch_or_the_short_hash() {
        assert_eq!(parse_head("ref: refs/heads/estabilizacion\n").as_deref(), Some("estabilizacion"));
        assert_eq!(parse_head("ref: refs/heads/feat/x").as_deref(), Some("feat/x"));
        assert_eq!(parse_head("a6cc857f00e1b2c3d4e5f60718293a4b5c6d7e8f\n").as_deref(), Some("a6cc857"));
        assert_eq!(parse_head(""), None);
    }

    #[test]
    fn porcelain_lines_are_counted() {
        assert_eq!(count_changes(" M src/app.rs\n?? docs/x.md\nR  a -> b\n"), 3);
        assert_eq!(count_changes(""), 0);
    }

    #[test]
    fn a_worktree_git_file_points_at_the_real_git_dir() {
        let base = std::env::temp_dir().join(format!("ankar-git-{}", std::process::id()));
        let (repo, wt) = (base.join("repo"), base.join("wt"));
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", repo.join(".git").display())).unwrap();
        assert_eq!(git_dir(&repo), Some(repo.join(".git")));
        assert_eq!(git_dir(&wt), Some(repo.join(".git")));
        assert_eq!(git_dir(&base), None);
        let _ = std::fs::remove_dir_all(&base);
    }
}
