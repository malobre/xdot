fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::process::Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let active_head_hash = String::from_utf8(output.stdout)?;

            println!("cargo:rustc-env=GIT_HEAD_HASH={}", active_head_hash);
        }
        Ok(output) => {
            println!(
                "cargo:warning={}",
                format_args!("Unable to retrieve HEAD hash: {}", output.status)
            );
        }
        Err(error) => {
            println!(
                "cargo:warning={}",
                format_args!("Unable to execute git command: {}", error)
            );
        }
    }

    Ok(())
}