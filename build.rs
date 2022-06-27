use std::process::Output;

use anyhow::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-changed=.git/packed-refs");

    match std::process::Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
    {
        Ok(Output { status, stdout, .. }) if status.success() => {
            println!(
                "cargo:rustc-env=GIT_HEAD_HASH={}",
                String::from_utf8(stdout)?
            );
        }
        Ok(Output { status, .. }) => {
            println!("cargo:warning=Unable to read HEAD symbolic ref: {status}");
        }
        Err(error) => {
            println!("cargo:warning=Unable to execute git command: {error}");
        }
    }

    Ok(())
}
