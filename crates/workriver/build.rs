// The commit this binary is built from, for `workriver status`: a CLI and a service built
// from different commits are told apart there.
use std::process::Command;

fn main() {
    let git = |args: &[&str]| Command::new("git").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let commit = git(&["describe", "--always", "--dirty", "--abbrev=9"]).filter(|c| !c.is_empty()).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=WORKRIVER_COMMIT={commit}");
    // Built again when HEAD moves or the index changes (a commit, a checkout).
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/index");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
