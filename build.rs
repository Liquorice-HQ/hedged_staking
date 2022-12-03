use std::process::Command;
use chrono::{DateTime, Utc};

fn main() {
    let git_revparse_output = Command::new("git").args(&["rev-parse", "HEAD"]).output().unwrap();
    let git_hash = String::from_utf8(git_revparse_output.stdout).unwrap();
    println!("cargo:rustc-env=BUILD_GIT_VERSION={}", git_hash);

    let git_status_output = Command::new("git").args(&["status", "--porcelain", "--untracked-files=no"]).output().unwrap();
    let git_dirty = if git_status_output.stdout.is_empty() { "" } else { "dirty" };
    println!("cargo:rustc-env=BUILD_GIT_DIRTY={}", git_dirty);

    let now: DateTime<Utc> = Utc::now();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", now.format("%Y-%m-%dT%H:%M"));
}
