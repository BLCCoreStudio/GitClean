use std::fs;
use std::path::Path;
use std::process::Command;

use gitclean::{apply, scan, CandidateStatus, GitCleanError};
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .expect("git must be available for safety tests");
    assert!(status.success(), "git command failed: {args:?}");
}

fn git_repo() -> TempDir {
    let temp = tempfile::tempdir().expect("create temp dir");
    git(temp.path(), &["init", "-q"]);
    temp
}

#[test]
fn dry_run_never_deletes_candidates() {
    let repo = git_repo();
    let target = repo.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("artifact.bin"), vec![0_u8; 4096]).unwrap();

    let report = scan(repo.path()).unwrap();

    assert!(target.exists());
    assert_eq!(report.safe_count(), 1);
    assert!(report.reclaimable_bytes() >= 4096);
}

#[test]
fn apply_deletes_untracked_generated_directory() {
    let repo = git_repo();
    let target = repo.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("artifact.bin"), vec![1_u8; 1024]).unwrap();

    let report = apply(repo.path()).unwrap();

    assert!(!target.exists());
    assert_eq!(report.deleted, vec![target]);
    assert!(report.freed_bytes >= 1024);
}

#[test]
fn apply_preserves_tracked_files_and_their_parent_candidate() {
    let repo = git_repo();
    let target = repo.path().join("target");
    fs::create_dir(&target).unwrap();
    let tracked = target.join("keep.txt");
    fs::write(&tracked, "important").unwrap();
    git(repo.path(), &["add", "target/keep.txt"]);

    let scan_report = scan(repo.path()).unwrap();
    let candidate = scan_report
        .candidates
        .iter()
        .find(|candidate| candidate.path == target)
        .unwrap();
    assert_eq!(
        candidate.status,
        CandidateStatus::Skipped("contains Git-tracked files".into())
    );

    let apply_report = apply(repo.path()).unwrap();
    assert!(apply_report.deleted.is_empty());
    assert_eq!(fs::read_to_string(tracked).unwrap(), "important");
}

#[test]
fn apply_never_deletes_candidate_containing_git_metadata() {
    let repo = git_repo();
    let target = repo.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::create_dir(target.join(".git")).unwrap();
    fs::write(target.join(".git/config"), "do not touch").unwrap();

    let report = apply(repo.path()).unwrap();

    assert!(report.deleted.is_empty());
    assert_eq!(
        fs::read_to_string(target.join(".git/config")).unwrap(),
        "do not touch"
    );
}

#[cfg(unix)]
#[test]
fn symlink_candidate_is_not_followed_or_deleted() {
    use std::os::unix::fs::symlink;

    let repo = git_repo();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    fs::write(&sentinel, "outside").unwrap();
    let link = repo.path().join("node_modules");
    symlink(outside.path(), &link).unwrap();

    let report = apply(repo.path()).unwrap();

    assert!(report.deleted.is_empty());
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "outside");
}

#[cfg(unix)]
#[test]
fn symlink_inside_candidate_does_not_delete_external_target() {
    use std::os::unix::fs::symlink;

    let repo = git_repo();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel.txt");
    fs::write(&sentinel, "outside").unwrap();

    let target = repo.path().join("target");
    fs::create_dir(&target).unwrap();
    symlink(&sentinel, target.join("external-link")).unwrap();

    let report = apply(repo.path()).unwrap();

    assert_eq!(report.deleted, vec![target]);
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "outside");
}

#[test]
fn apply_requires_git_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();

    let error = apply(temp.path()).unwrap_err();

    assert!(matches!(error, GitCleanError::GitRequired));
    assert!(target.exists());
}

#[test]
fn ambiguous_build_directory_requires_gitignore() {
    let repo = git_repo();
    let build = repo.path().join("build");
    fs::create_dir(&build).unwrap();
    fs::write(build.join("artifact"), "generated").unwrap();

    let first = apply(repo.path()).unwrap();
    assert!(first.deleted.is_empty());
    assert!(build.exists());

    fs::write(repo.path().join(".gitignore"), "build/\n").unwrap();
    let second = apply(repo.path()).unwrap();
    assert_eq!(second.deleted, vec![build.clone()]);
    assert!(!build.exists());
}

#[test]
fn target_path_inside_dot_git_is_rejected() {
    let repo = git_repo();
    let git_dir = repo.path().join(".git");

    let error = scan(&git_dir).unwrap_err();

    assert!(matches!(error, GitCleanError::UnsafeTarget(_)));
}

#[cfg(unix)]
#[test]
fn symlink_as_requested_target_is_rejected() {
    use std::os::unix::fs::symlink;

    let repo = git_repo();
    let alias_parent = tempfile::tempdir().unwrap();
    let alias = alias_parent.path().join("repo-link");
    symlink(repo.path(), &alias).unwrap();

    let error = scan(alias).unwrap_err();

    assert!(matches!(error, GitCleanError::UnsafeTarget(_)));
}

#[test]
fn filesystem_root_is_rejected() {
    let error = scan(Path::new("/")).unwrap_err();
    assert!(matches!(error, GitCleanError::UnsafeTarget(_)));
}

#[test]
fn home_directory_is_rejected() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let error = scan(std::path::PathBuf::from(home)).unwrap_err();
    assert!(matches!(error, GitCleanError::UnsafeTarget(_)));
}
