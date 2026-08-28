use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use gitclean::{apply, human_bytes, scan, CandidateStatus};

#[derive(Debug, Parser)]
#[command(
    name = "gitclean",
    version,
    about = "Safely find generated build/cache directories. Dry-run by default."
)]
struct Cli {
    /// Project directory to inspect.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Delete only candidates that pass all safety checks.
    #[arg(long)]
    apply: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = if cli.apply {
        run_apply(cli.path)
    } else {
        run_dry(cli.path)
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_dry(path: PathBuf) -> Result<(), gitclean::GitCleanError> {
    let report = scan(path)?;
    println!("GitClean dry-run — nothing will be deleted");
    println!("Target: {}", report.target.display());
    print_candidates(&report);
    println!();
    println!(
        "Reclaimable: {} across {} safe director{}",
        human_bytes(report.reclaimable_bytes()),
        report.safe_count(),
        if report.safe_count() == 1 { "y" } else { "ies" }
    );
    println!("Run again with --apply to delete only SAFE entries.");
    Ok(())
}

fn run_apply(path: PathBuf) -> Result<(), gitclean::GitCleanError> {
    let report = apply(path)?;
    println!("GitClean apply — explicit deletion enabled");
    println!("Target: {}", report.scan.target.display());

    if report.deleted.is_empty() {
        println!("No safe generated directories were deleted.");
    } else {
        for path in &report.deleted {
            println!("DELETED  {}", display_relative(&report.scan.target, path));
        }
        println!();
        println!(
            "Freed approximately {} across {} director{}.",
            human_bytes(report.freed_bytes),
            report.deleted.len(),
            if report.deleted.len() == 1 {
                "y"
            } else {
                "ies"
            }
        );
    }

    let skipped = report
        .scan
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.status, CandidateStatus::Skipped(_)))
        .count();
    if skipped > 0 {
        println!("Skipped {skipped} candidate(s) that did not pass safety checks.");
    }

    Ok(())
}

fn print_candidates(report: &gitclean::ScanReport) {
    if report.candidates.is_empty() {
        println!("No known generated directories found.");
        return;
    }

    for candidate in &report.candidates {
        let relative = display_relative(&report.target, &candidate.path);
        match &candidate.status {
            CandidateStatus::Safe => {
                println!(
                    "SAFE  {:>10}  {relative}",
                    human_bytes(candidate.size_bytes)
                );
            }
            CandidateStatus::Skipped(reason) => {
                println!(
                    "SKIP  {:>10}  {relative} — {reason}",
                    human_bytes(candidate.size_bytes)
                );
            }
        }
    }
}

fn display_relative(root: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
