//! Git facts for the shell footer and the changes pane: the branch from `.git/HEAD`, changed
//! files with their +/- line counts, and unified diffs. Every `git` call runs off the UI thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Branch name from the contents of `.git/HEAD`; a detached HEAD shows the short hash.
pub fn parse_head(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        return Some(branch.to_owned());
    }
    (head.len() >= 7 && head.bytes().all(|b| b.is_ascii_hexdigit())).then(|| head[..7].to_owned())
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

/// A `git` command in `root` with no console window.
pub fn command(root: &Path, args: &[&str]) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(root).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// stdout of a successful `git` run; `None` when git is missing or fails.
pub fn run(root: &Path, args: &[&str]) -> Option<String> {
    let out = command(root, args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One changed file. `added`/`removed` are `None` for binary files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    /// Repo-relative, with `/` separators as git prints it.
    pub path: String,
    /// Porcelain XY code: ` M`, `A `, `??`, ...
    pub code: String,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

/// `git status --porcelain -z --no-renames`: `XY path\0` records -> (code, path).
pub fn parse_porcelain_z(text: &str) -> Vec<(String, String)> {
    text.split('\0')
        .filter(|r| r.len() > 3)
        .map(|r| (r[..2].to_owned(), r[3..].to_owned()))
        .collect()
}

/// `git diff --numstat -z --no-renames`: `added\tremoved\tpath\0`; binary files show `-\t-`.
pub fn parse_numstat_z(text: &str) -> HashMap<String, (Option<u32>, Option<u32>)> {
    text.split('\0')
        .filter_map(|r| {
            let mut f = r.splitn(3, '\t');
            let (a, d, path) = (f.next()?, f.next()?, f.next()?);
            Some((path.to_owned(), (a.parse().ok(), d.parse().ok())))
        })
        .collect()
}

/// Lines in an untracked file (every line is an addition); `None` when it looks binary.
fn count_lines(path: &Path) -> Option<u32> {
    const CAP: u64 = 4 << 20;
    if std::fs::metadata(path).ok()?.len() > CAP {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let n = bytes.iter().filter(|&&b| b == b'\n').count();
    Some((n + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"))) as u32)
}

/// Every changed file under `root` against HEAD (staged, unstaged and untracked), sorted.
pub fn changes(root: &Path) -> Option<Vec<FileChange>> {
    let status = run(root, &["status", "--porcelain", "-z", "--untracked-files=all", "--no-renames"])?;
    // No commit yet: HEAD does not resolve and every file is new.
    let numstat = run(root, &["diff", "--numstat", "-z", "--no-renames", "HEAD"]).unwrap_or_default();
    let stats = parse_numstat_z(&numstat);
    let mut out: Vec<FileChange> = parse_porcelain_z(&status)
        .into_iter()
        .map(|(code, path)| {
            let (added, removed) = if code == "??" {
                let n = count_lines(&root.join(&path));
                (n, n.map(|_| 0))
            } else {
                stats.get(&path).copied().unwrap_or((Some(0), Some(0)))
            };
            FileChange { path, code, added, removed }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Some(out)
}

/// (files, lines added, lines removed); binary files count as files only.
pub fn totals(changes: &[FileChange]) -> (usize, u32, u32) {
    let sum = |f: fn(&FileChange) -> Option<u32>| changes.iter().filter_map(f).sum();
    (changes.len(), sum(|c| c.added), sum(|c| c.removed))
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
    fn porcelain_z_records_keep_spaces_in_paths() {
        let text = " M src/app.rs\0?? docs/my notes.md\0A  new.rs\0";
        assert_eq!(
            parse_porcelain_z(text),
            vec![
                (" M".into(), "src/app.rs".into()),
                ("??".into(), "docs/my notes.md".into()),
                ("A ".into(), "new.rs".into()),
            ]
        );
        assert!(parse_porcelain_z("").is_empty());
    }

    #[test]
    fn numstat_counts_lines_and_marks_binaries() {
        let stats = parse_numstat_z("12\t3\tsrc/app.rs\0-\t-\tassets/logo.png\x000\t7\told name.txt\0");
        assert_eq!(stats["src/app.rs"], (Some(12), Some(3)));
        assert_eq!(stats["assets/logo.png"], (None, None), "binary");
        assert_eq!(stats["old name.txt"], (Some(0), Some(7)));
        assert_eq!(stats.len(), 3);
    }

    #[test]
    fn totals_add_up_text_files_and_count_binaries_as_files() {
        let f = |path: &str, added, removed| FileChange { path: path.into(), code: " M".into(), added, removed };
        let all = [f("a", Some(5), Some(1)), f("b.png", None, None), f("c", Some(0), Some(4))];
        assert_eq!(totals(&all), (3, 5, 5));
    }

    #[test]
    fn untracked_text_counts_its_lines_and_binary_has_none() {
        let dir = std::env::temp_dir().join(format!("ankar-lines-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "one\ntwo\nthree").unwrap();
        std::fs::write(dir.join("b.bin"), [1u8, 0, 2]).unwrap();
        std::fs::write(dir.join("c.txt"), "").unwrap();
        assert_eq!(count_lines(&dir.join("a.txt")), Some(3));
        assert_eq!(count_lines(&dir.join("b.bin")), None);
        assert_eq!(count_lines(&dir.join("c.txt")), Some(0));
        let _ = std::fs::remove_dir_all(&dir);
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
