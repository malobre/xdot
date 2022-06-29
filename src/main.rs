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
        let mut verbosity = 0;
        let mut unlink = false;
        let mut dry_run = false;

        let mut parser = lexopt::Parser::from_env();

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Long("dry-run") => dry_run = true,
                Arg::Long("unlink") => unlink = true,
                Arg::Long("verbose") | Arg::Short('v') => {
                    verbosity += 1;
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

    for package_name in &args.packages {
        let package_path =
            PathBuf::from_iter([&home, Path::new(".xdot"), Path::new(&package_name)]);

        println!(
            "{} config for `{}` ({})",
            if args.unlink { "Unlinking" } else { "Linking" },
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

            symlink_or_descend(&original.path(), &link, &args)?;
        }
    }

    Ok(())
}

fn symlink_or_descend(original: &Path, link: &Path, args: &Args) -> Result<()> {
    match (link.metadata(), original.metadata()) {
        (Ok(a), Ok(b)) if a.ino() == b.ino() && a.dev() == b.dev() => {
            if args.unlink {
                if args.dry_run {
                    println!("[DRY RUN] Removing symlink: {}", link.display());
                    return Ok(());
                }

                println!("Removing symlink: {}", link.display());
                std::fs::remove_file(link).context("Unable to remove symlink")?;
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
                if args.dry_run {
                    println!("[DRY RUN] {} => {}", link.display(), original.display());
                } else {
                    println!("{} => {}", link.display(), original.display());

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
