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
    verbosity: u8,
    unlink: bool,
    dry_run: bool,
}

impl Args {
    fn from_env() -> Result<Self> {
        let mut packages = Vec::new();
        let mut verbosity = 0_u8;
        let mut unlink = false;
        let mut dry_run = false;

        let mut parser = lexopt::Parser::from_env();

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Long("dry-run") => dry_run = true,
                Arg::Long("unlink") => unlink = true,
                Arg::Long("verbose") | Arg::Short('v') => {
                    verbosity = verbosity.saturating_add(1);
                }
                Arg::Long("help") | Arg::Short('h') => {
                    println!(concat!(
                        "Usage: xdot [options] [package...]\n",
                        "Symlink your dotfiles from `~/.xdot`.\n\n",
                        "Options:\n",
                        "  --unlink\tRemove symlinks.\n",
                        "  --dry-run\tDon't modify the file system.\n",
                        "  -v, --verbose\tIncrease verbosity.\n",
                        "  -h, --help\tShow this help message and exit.\n",
                        "  --version\tShow version information and exit.\n",
                    ));
                    std::process::exit(0);
                }
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

        Ok(Self {
            packages,
            verbosity,
            unlink,
            dry_run,
        })
    }
}

fn main() -> Result<()> {
    let home = match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).into_boxed_path(),
        None => bail!("$HOME is not set"),
    };

    let args = Args::from_env()?;

    if args.packages.is_empty() {
        bail!("No packages specified");
    }

    if args.dry_run {
        println!("Dry run mode, no changes will be made.");
    }

    for package in &args.packages {
        let package_path = PathBuf::from_iter([&home, Path::new(".xdot"), Path::new(&package)]);

        println!(
            "{} config for `{}` ({})",
            if args.unlink { "Unlinking" } else { "Linking" },
            package.to_string_lossy(),
            package_path.display()
        );

        for original in package_path
            .read_dir()
            .context("Unable to read package content")?
        {
            let original = original?;

            if let Some(env_var_name) = strip_at_sign_prefix(&original.file_name()) {
                let link = match (std::env::var_os(env_var_name), env_var_name.to_str()) {
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
                };

                let original = original.path();

                for entry in original
                    .read_dir()
                    .with_context(|| format!("Unable to descend into {}", original.display()))?
                {
                    let entry = entry?;

                    symlink_or_descend(&entry.path(), &link.join(entry.file_name()), &args)?;
                }
            } else {
                symlink_or_descend(
                    &original.path(),
                    &PathBuf::from_iter([
                        Path::new("/"),
                        original.path().strip_prefix(&package_path)?,
                    ]),
                    &args,
                )?;
            }
        }
    }

    Ok(())
}

fn strip_at_sign_prefix(file_name: &OsStr) -> Option<&OsStr> {
    let file_name = file_name.as_bytes();

    if file_name[0] == b'@' {
        Some(OsStr::from_bytes(&file_name[1..]))
    } else {
        None
    }
}

fn symlink_or_descend(original: &Path, link: &Path, args: &Args) -> Result<()> {
    match (link.metadata(), original.metadata()) {
        (Ok(a), Ok(b)) if a.ino() == b.ino() && a.dev() == b.dev() => {
            if args.unlink {
                println!("Removing symlink: {}", link.display());

                if !args.dry_run {
                    std::fs::remove_file(link).context("Unable to remove symlink")?;
                }
            } else if args.verbosity > 0 {
                println!("Skipping preexisting symlink: {}", link.display());
            }

            Ok(())
        }
        (Ok(link_metadata), _) => {
            if link_metadata.is_file() {
                bail!("{} already exists", link.display());
            }

            if args.verbosity > 0 {
                println!("Descending into preexisting directory: {}", link.display());
            }

            for entry in original
                .read_dir()
                .with_context(|| format!("Unable to descend into {}", original.display()))?
            {
                let entry = entry?;

                symlink_or_descend(&entry.path(), &link.join(entry.file_name()), args)?;
            }

            Ok(())
        }
        _ => {
            if !args.unlink {
                println!("{} => {}", link.display(), original.display());

                if !args.dry_run {
                    symlink(&original, &link).with_context(|| {
                        format!(
                            "Unable to symlink {} => {}",
                            link.display(),
                            original.display()
                        )
                    })?;
                }
            } else if args.verbosity > 0 {
                println!("Skipping non-existent file: {}", link.display());
            }

            Ok(())
        }
    }
}
