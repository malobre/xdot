#[cfg(not(target_family = "unix"))]
compile_error!("`xdot` only supports Unix.");

use std::{
    ffi::OsStr,
    os::unix::{
        ffi::OsStrExt,
        fs::{symlink, MetadataExt},
    },
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

/// Flattens literals into a single static string slice, placing a newline between each element.
macro_rules! joinln {
    ($head:expr, $($e:expr),* $(,)?) => {
        concat!($head, $('\n', $e, )*)
    };
}

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
            use lexopt::Arg;

            match arg {
                Arg::Long("dry-run") => dry_run = true,
                Arg::Long("unlink") => unlink = true,
                Arg::Long("verbose") | Arg::Short('v') => {
                    verbosity = verbosity.saturating_add(1);
                }
                Arg::Long("help") | Arg::Short('h') => {
                    println!(joinln!(
                        "Usage: xdot [options] [--] package...",
                        "Symlink your dotfiles from `~/.xdot`.",
                        "",
                        "Options:",
                        "  --unlink       Remove symlinks.",
                        "  --dry-run      Don't modify the file system.",
                        "  -v, --verbose  Increase verbosity.",
                        "  -h, --help     Show this help message and exit.",
                        "  --version      Show version information and exit.",
                    ));

                    std::process::exit(0);
                }
                Arg::Long("version") => {
                    let version = env!("CARGO_PKG_VERSION");

                    if let Some(hash) = option_env!("GIT_HEAD_HASH") {
                        println!("xdot {version} ({hash})");
                    } else {
                        println!("xdot {version}");
                    }

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

    let default_xdg_data_home = home.join(".local/share").into_boxed_path();
    let default_xdg_state_home = home.join(".local/state").into_boxed_path();
    let default_xdg_cache_home = home.join(".cache").into_boxed_path();
    let default_xdg_config_home = home.join(".config").into_boxed_path();

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
                let link = std::env::var_os(env_var_name).map(PathBuf::from);

                let link = match (link.as_deref(), env_var_name.to_str()) {
                    (Some(value), _) => value,
                    (None, Some("XDG_DATA_HOME")) => &*default_xdg_data_home,
                    (None, Some("XDG_STATE_HOME")) => &*default_xdg_state_home,
                    (None, Some("XDG_CACHE_HOME")) => &*default_xdg_cache_home,
                    (None, Some("XDG_CONFIG_HOME")) => &*default_xdg_config_home,
                    (None, _) => bail!(
                        "Unable to find environment variable `{}`",
                        env_var_name.to_string_lossy()
                    ),
                };

                descend_and_symlink(&original.path(), link, &args)?;
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

fn descend_and_symlink(original: &Path, link: &Path, args: &Args) -> Result<()> {
    for entry in original
        .read_dir()
        .with_context(|| format!("Unable to descend into {}", original.display()))?
    {
        let entry = entry?;

        symlink_or_descend(&entry.path(), &link.join(entry.file_name()), args)?;
    }

    Ok(())
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

            descend_and_symlink(original, link, args)?;

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
