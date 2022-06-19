use std::{
    ffi::OsStr,
    os::unix::{
        fs::symlink,
        prelude::{MetadataExt, OsStrExt},
    },
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use lexopt::Arg;

#[cfg(not(target_family = "unix"))]
compile_error!("`xdot` only supports Unix.");

struct Args {
    packages: Vec<Box<OsStr>>,
}

impl Args {
    fn from_env() -> Result<Self> {
        let mut packages = Vec::new();

        let mut parser = lexopt::Parser::from_env();

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Long("version") => {
                    println!("xdot {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                Arg::Value(package) => {
                    packages.push(package.into_boxed_os_str());
                }
                _ => bail!(arg.unexpected()),
            }
        }

        Ok(Self { packages })
    }
}

fn main() -> Result<()> {
    let home = match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).into_boxed_path(),
        None => bail!("$HOME is not set"),
    };

    let args = Args::from_env()?;

    for package_name in &args.packages {
        let package_path =
            PathBuf::from_iter([&home, Path::new(".xdot"), Path::new(&package_name)]);

        println!(
            "Installing config for `{}` ({})",
            package_name.to_string_lossy(),
            package_path.display()
        );

        for original in package_path
            .read_dir()
            .context("Unable to read package content")?
        {
            let original = original?;
            let original_file_name = original.file_name();
            let original_file_name = original_file_name.as_bytes();

            let link = if original_file_name[0] == b'@' {
                let env_var_name = OsStr::from_bytes(&original_file_name[1..]);

                match (std::env::var_os(env_var_name), env_var_name.to_str()) {
                    (Some(value), _) => PathBuf::from(value),
                    (None, Some("XDG_DATA_HOME")) => home.join(".local/share"),
                    (None, Some("XDG_CONFIG_HOME")) => home.join(".config"),
                    (None, Some("XDG_STATE_HOME")) => home.join(".local/state"),
                    (None, Some("XDG_CACHE_HOME")) => home.join(".cache"),
                    (None, _) => {
                        bail!(
                            "Unable to find environment variable `{}`",
                            env_var_name.to_string_lossy()
                        )
                    }
                }
            } else {
                PathBuf::from_iter([Path::new("/"), original.path().strip_prefix(&package_path)?])
            };

            symlink_or_descend(&original.path(), &link)?;
        }
    }

    Ok(())
}

fn symlink_or_descend(original: &Path, link: &Path) -> Result<()> {
    match (link.metadata(), original.metadata()) {
        (Ok(a), Ok(b)) if a.ino() == b.ino() && a.dev() == b.dev() => Ok(()),
        (Ok(link_metadata), _) => {
            if link_metadata.is_file() {
                bail!("{} already exists", link.display());
            }

            for entry in original
                .read_dir()
                .with_context(|| format!("Unable to descend into `{}`", original.display()))?
            {
                let entry = entry?;

                symlink_or_descend(&entry.path(), &link.join(entry.file_name()))?;
            }

            Ok(())
        }
        _ => {
            println!("{} => {}", link.display(), original.display());

            symlink(&original, &link).with_context(|| {
                format!(
                    "Unable to symlink {} => {}",
                    link.display(),
                    original.display()
                )
            })
        }
    }
}
