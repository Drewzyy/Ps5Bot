// Embeds the git commit and build time so the app can tell whether a newer build exists.
// CI sets GITHUB_SHA (and GITHUB_REF_NAME); locally we ask git. Never fails the build.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    if !o.status.success() {
        return None;
    }
    let s = String::from_utf8(o.stdout).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn main() {
    let sha = std::env::var("GITHUB_SHA").ok().filter(|s| !s.is_empty()).or_else(|| git(&["rev-parse", "HEAD"])).unwrap_or_else(|| "unknown".into());
    let branch = std::env::var("GITHUB_REF_NAME").ok().filter(|s| !s.is_empty()).or_else(|| git(&["rev-parse", "--abbrev-ref", "HEAD"])).unwrap_or_default();
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    println!("cargo:rustc-env=CT_GIT_SHA={sha}");
    println!("cargo:rustc-env=CT_GIT_BRANCH={branch}");
    println!("cargo:rustc-env=CT_BUILD_UNIX={secs}");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
}
