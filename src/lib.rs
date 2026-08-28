use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use walkdir::WalkDir;

#[derive(Debug)]
pub enum GitCleanError {
    Io(io::Error),
    UnsafeTarget(String),
    GitRequired,
    GitCommand(String),
}

impl fmt::Display for GitCleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::UnsafeTarget(reason) => write!(f, "refusing unsafe target: {reason}"),
            Self::GitRequired => write!(
                f,
                "--apply requires a Git worktree so tracked files can be protected"
            ),
            Self::GitCommand(message) => write!(f, "Git safety check failed: {message}"),
        }
    }
}

impl std::error::Error for GitCleanError {}

impl From<io::Error> for GitCleanError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Generated,
    AmbiguousGenerated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateStatus {
    Safe,
    Skipped(String),
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub kind: CandidateKind,
    pub status: CandidateStatus,
}

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub target: PathBuf,
    pub git_root: Option<PathBuf>,
    pub candidates: Vec<Candidate>,
}

impl ScanReport {
    pub fn reclaimable_bytes(&self) -> u64 {
        self.candidates
            .iter()
            .filter(|candidate| candidate.status == CandidateStatus::Safe)
            .map(|candidate| candidate.size_bytes)
            .sum()
    }

    pub fn safe_count(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| candidate.status == CandidateStatus::Safe)
            .count()
    }
}

#[derive(Debug, Clone)]
pub struct ApplyReport {
    pub scan: ScanReport,
    pub deleted: Vec<PathBuf>,
    pub freed_bytes: u64,
}

pub fn scan(target: impl AsRef<Path>) -> Result<ScanReport, GitCleanError> {
    let target = validate_target(target.as_ref())?;
    let git_root = find_git_root(&target)?;
    let tracked = if let Some(root) = &git_root {
        tracked_paths(root)?
    } else {
        HashSet::new()
    };

    let mut candidates = Vec::new();
    let mut walker = WalkDir::new(&target)
        .follow_links(false)
        .min_depth(1)
        .into_iter();

    while let Some(entry_result) = walker.next() {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(err) => {
                return Err(GitCleanError::Io(io::Error::other(err.to_string())));
            }
        };

        let name = entry.file_name().to_string_lossy();

        if name == ".git" {
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }

        let Some(kind) = classify_candidate(&name) else {
            continue;
        };

        let path = entry.path().to_path_buf();

        if entry.file_type().is_symlink() {
            candidates.push(Candidate {
                path,
                size_bytes: 0,
                kind,
                status: CandidateStatus::Skipped("symlink (never followed)".into()),
            });
            continue;
        }

        if !entry.file_type().is_dir() {
            continue;
        }

        let (size_bytes, contains_git) = inspect_candidate(&path)?;
        let status = candidate_status(&path, kind, git_root.as_deref(), &tracked, contains_git)?;
        let is_safe = status == CandidateStatus::Safe;

        candidates.push(Candidate {
            path,
            size_bytes,
            kind,
            status,
        });

        if is_safe {
            walker.skip_current_dir();
        }
    }

    candidates.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(ScanReport {
        target,
        git_root,
        candidates,
    })
}

pub fn apply(target: impl AsRef<Path>) -> Result<ApplyReport, GitCleanError> {
    let scan = scan(target)?;
    let git_root = scan.git_root.clone().ok_or(GitCleanError::GitRequired)?;
    let mut deleted = Vec::new();
    let mut freed_bytes = 0_u64;

    for candidate in &scan.candidates {
        if candidate.status != CandidateStatus::Safe {
            continue;
        }

        revalidate_before_delete(&scan.target, &git_root, candidate)?;
        fs::remove_dir_all(&candidate.path)?;
        deleted.push(candidate.path.clone());
        freed_bytes = freed_bytes.saturating_add(candidate.size_bytes);
    }

    Ok(ApplyReport {
        scan,
        deleted,
        freed_bytes,
    })
}

fn validate_target(input: &Path) -> Result<PathBuf, GitCleanError> {
    let metadata = fs::symlink_metadata(input)?;
    if metadata.file_type().is_symlink() {
        return Err(GitCleanError::UnsafeTarget(
            "the requested target is a symlink".into(),
        ));
    }
    if !metadata.is_dir() {
        return Err(GitCleanError::UnsafeTarget(
            "the requested target is not a directory".into(),
        ));
    }

    let canonical = fs::canonicalize(input)?;

    if canonical.parent().is_none() {
        return Err(GitCleanError::UnsafeTarget(
            "filesystem root cannot be scanned".into(),
        ));
    }

    if canonical
        .components()
        .any(|component| matches!(component, Component::Normal(name) if name == std::ffi::OsStr::new(".git")))
    {
        return Err(GitCleanError::UnsafeTarget(
            "paths inside .git are forbidden".into(),
        ));
    }

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        if let Ok(canonical_home) = fs::canonicalize(home) {
            if canonical == canonical_home {
                return Err(GitCleanError::UnsafeTarget(
                    "the home directory cannot be scanned".into(),
                ));
            }
        }
    }

    Ok(canonical)
}

fn classify_candidate(name: &str) -> Option<CandidateKind> {
    let generated = matches!(
        name,
        "target"
            | "node_modules"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".tox"
            | ".nox"
            | ".next"
            | ".nuxt"
            | ".parcel-cache"
            | ".turbo"
            | ".vite"
            | ".gradle"
    ) || name.starts_with("cmake-build-");

    if generated {
        return Some(CandidateKind::Generated);
    }

    if matches!(name, "build" | "dist" | "out" | "coverage") {
        return Some(CandidateKind::AmbiguousGenerated);
    }

    None
}

fn inspect_candidate(path: &Path) -> Result<(u64, bool), GitCleanError> {
    let mut size = 0_u64;
    let mut contains_git = false;
    let mut walker = WalkDir::new(path).follow_links(false).into_iter();

    while let Some(entry_result) = walker.next() {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(err) => {
                return Err(GitCleanError::Io(io::Error::other(err.to_string())));
            }
        };

        if entry.depth() > 0 && entry.file_name() == std::ffi::OsStr::new(".git") {
            contains_git = true;
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }

        if entry.file_type().is_file() {
            size = size.saturating_add(fs::metadata(entry.path())?.len());
        }
    }

    Ok((size, contains_git))
}

fn candidate_status(
    path: &Path,
    kind: CandidateKind,
    git_root: Option<&Path>,
    tracked: &HashSet<PathBuf>,
    contains_git: bool,
) -> Result<CandidateStatus, GitCleanError> {
    if contains_git {
        return Ok(CandidateStatus::Skipped("contains .git metadata".into()));
    }

    let Some(git_root) = git_root else {
        return Ok(CandidateStatus::Skipped(
            "not inside a Git worktree; tracked-file safety unavailable".into(),
        ));
    };

    if tracked
        .iter()
        .any(|tracked_path| tracked_path.starts_with(path))
    {
        return Ok(CandidateStatus::Skipped(
            "contains Git-tracked files".into(),
        ));
    }

    if kind == CandidateKind::AmbiguousGenerated && !git_ignored(git_root, path)? {
        return Ok(CandidateStatus::Skipped(
            "ambiguous directory name and not ignored by Git".into(),
        ));
    }

    Ok(CandidateStatus::Safe)
}

fn find_git_root(target: &Path) -> Result<Option<PathBuf>, GitCleanError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(target)
        .args(["rev-parse", "--show-toplevel"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output();

    let output = match output {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(GitCleanError::Io(err)),
    };

    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let root = PathBuf::from(text.trim());
    let root = fs::canonicalize(root)?;

    if !target.starts_with(&root) {
        return Err(GitCleanError::GitCommand(
            "Git reported a worktree root that does not contain the target".into(),
        ));
    }

    Ok(Some(root))
}

fn tracked_paths(git_root: &Path) -> Result<HashSet<PathBuf>, GitCleanError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(git_root)
        .args(["ls-files", "-z"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()?;

    if !output.status.success() {
        return Err(GitCleanError::GitCommand(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }

    let mut paths = HashSet::new();
    for raw_path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
    {
        let relative = path_from_git_bytes(raw_path);
        paths.insert(git_root.join(relative));
    }
    Ok(paths)
}

#[cfg(unix)]
fn path_from_git_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn path_from_git_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

fn git_ignored(git_root: &Path, path: &Path) -> Result<bool, GitCleanError> {
    let relative = path.strip_prefix(git_root).map_err(|_| {
        GitCleanError::GitCommand("candidate escaped the Git worktree".into())
    })?;

    let status = Command::new("git")
        .arg("-C")
        .arg(git_root)
        .args(["check-ignore", "-q", "--"])
        .arg(relative)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .status()?;

    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(code) => Err(GitCleanError::GitCommand(format!(
            "git check-ignore exited with status {code}"
        ))),
        None => Err(GitCleanError::GitCommand(
            "git check-ignore terminated unexpectedly".into(),
        )),
    }
}

fn revalidate_before_delete(
    target: &Path,
    git_root: &Path,
    candidate: &Candidate,
) -> Result<(), GitCleanError> {
    if !candidate.path.starts_with(target) || candidate.path == target {
        return Err(GitCleanError::UnsafeTarget(format!(
            "candidate escaped target: {}",
            candidate.path.display()
        )));
    }

    if candidate
        .path
        .components()
        .any(|component| matches!(component, Component::Normal(name) if name == std::ffi::OsStr::new(".git")))
    {
        return Err(GitCleanError::UnsafeTarget(
            "candidate path contains .git".into(),
        ));
    }

    let metadata = fs::symlink_metadata(&candidate.path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(GitCleanError::UnsafeTarget(format!(
            "candidate changed type before deletion: {}",
            candidate.path.display()
        )));
    }

    let (_, contains_git) = inspect_candidate(&candidate.path)?;
    if contains_git {
        return Err(GitCleanError::UnsafeTarget(format!(
            "candidate gained .git metadata before deletion: {}",
            candidate.path.display()
        )));
    }

    let tracked = tracked_paths(git_root)?;
    if tracked
        .iter()
        .any(|tracked_path| tracked_path.starts_with(&candidate.path))
    {
        return Err(GitCleanError::UnsafeTarget(format!(
            "candidate gained Git-tracked files before deletion: {}",
            candidate.path.display()
        )));
    }

    if candidate.kind == CandidateKind::AmbiguousGenerated
        && !git_ignored(git_root, &candidate.path)?
    {
        return Err(GitCleanError::UnsafeTarget(format!(
            "ambiguous candidate is no longer ignored by Git: {}",
            candidate.path.display()
        )));
    }

    Ok(())
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0_usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
