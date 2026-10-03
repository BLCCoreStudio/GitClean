use gitclean::apply;
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .expect("git must be available for safety tests");
    assert!(status.success(), "git command failed: {args:?}");
}

#[test]
fn apply_preserves_candidate_containing_gitfile_metadata() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);

    let target = repo.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join(".git"), "gitdir: ../somewhere\n").unwrap();
    fs::write(target.join("artifact.bin"), vec![1_u8; 1024]).unwrap();

    let report = apply(repo.path()).unwrap();

    assert!(report.deleted.is_empty());
    assert!(target.exists());
    assert_eq!(
        fs::read_to_string(target.join(".git")).unwrap(),
        "gitdir: ../somewhere\n"
    );
}
