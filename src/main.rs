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

use anyhow::{anyhow, bail, Context, Result};
use ignore::WalkBuilder;

/// Flattens literals into a single static string slice, placing a newline between each element.
macro_rules! joinln {
    ($head:expr, $($e:expr),* $(,)?) => {
        concat!($head, $('\n', $e, )*)
    };
}

enum PackageSpec {
    None,
    All,
    List(Vec<Box<OsStr>>),
}

struct Options {
    verbosity: u8,
    unlink: bool,
    dry_run: bool,
}

struct Args {
    package_spec: PackageSpec,
    options: Options,
}

// Better to be explicit
#[allow(clippy::derivable_impls)]
impl Default for Args {
    fn default() -> Self {
        Self {
            package_spec: PackageSpec::None,
            options: Options {
                verbosity: 0,
                unlink: false,
                dry_run: false,
            },
        }
    }
}

impl Args {
    fn from_env() -> Result<Self> {
        let mut args = Self::default();

        let mut parser = lexopt::Parser::from_env();

        while let Some(arg) = parser.next()? {
            use lexopt::Arg;

            match arg {
                Arg::Long("dry-run") => args.options.dry_run = true,
                Arg::Long("unlink") => args.options.unlink = true,
                Arg::Long("verbose") | Arg::Short('v') => {
                    args.options.verbosity = args.options.verbosity.saturating_add(1);
                }
                Arg::Long("help") | Arg::Short('h') => {
                    println!(joinln!(
                        "Usage: xdot [options] [--] [package...]",
                        "Symlink your dotfiles from `~/.xdot`.",
                        "",
                        "Options:",
                        "  --all          Symlink all packages.",
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
                Arg::Value(package) => match args.package_spec {
                    PackageSpec::All => {
                        bail!("Cannot specify packages after `--all`");
                    }
                    PackageSpec::None => {
                        args.package_spec = PackageSpec::List(vec![package.into_boxed_os_str()]);
                    }
                    PackageSpec::List(ref mut list) => list.push(package.into_boxed_os_str()),
                },
                Arg::Long("all") => {
                    if let PackageSpec::List(_) = args.package_spec {
                        bail!("Cannot specify `--all` after explicit packages");
                    }

                    args.package_spec = PackageSpec::All;
                }
                _ => bail!(arg.unexpected()),
            }
        }

        Ok(args)
    }
}

fn main() -> Result<()> {
    let home = match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).into_boxed_path(),
        None => bail!("$HOME is not set"),
    };

    let Args {
        package_spec,
        options,
    } = Args::from_env()?;

    if matches!(package_spec, PackageSpec::None) {
        bail!("No packages specified");
    }

    if options.dry_run {
        println!("Dry run mode, no changes will be made.");
    }

    let default_xdg_data_home = home.join(".local/share").into_boxed_path();
    let default_xdg_state_home = home.join(".local/state").into_boxed_path();
    let default_xdg_cache_home = home.join(".cache").into_boxed_path();
    let default_xdg_config_home = home.join(".config").into_boxed_path();

    let packages_root = PathBuf::from_iter([&home, Path::new(".xdot")]).into_boxed_path();

    let packages = match package_spec {
        PackageSpec::None => unreachable!(),
        PackageSpec::All => WalkBuilder::new(&packages_root)
            .require_git(true)
            .hidden(true)
            .parents(true)
            .ignore(true)
            .git_global(true)
            .git_ignore(true)
            .git_exclude(true)
            .max_depth(Some(1))
            .follow_links(false)
            .filter_entry(
                |entry| matches!(entry.file_type(), Some(file_type) if file_type.is_dir()),
            )
            .build()
            .skip(1)
            .map(|entry| entry.map(|entry| entry.file_name().to_owned().into_boxed_os_str()))
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("Unable to list packages ({})", packages_root.display()))?
            .into_boxed_slice(),
        PackageSpec::List(list) => list.into_boxed_slice(),
    };

    for package in packages.iter() {
        let package_path =
            PathBuf::from_iter([&packages_root, Path::new(&package)]).into_boxed_path();

        println!(
            "{} config for `{}` ({})",
            if options.unlink {
                "Unlinking"
            } else {
                "Linking"
            },
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

                let link = link
                    .as_deref()
                    .or_else(|| match env_var_name.to_str() {
                        Some("XDG_DATA_HOME") => Some(&*default_xdg_data_home),
                        Some("XDG_STATE_HOME") => Some(&*default_xdg_state_home),
                        Some("XDG_CACHE_HOME") => Some(&*default_xdg_cache_home),
                        Some("XDG_CONFIG_HOME") => Some(&*default_xdg_config_home),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        anyhow!(
                            "Unable to find environment variable `{}`",
                            env_var_name.to_string_lossy()
                        )
                    })?;

                descend_and_symlink(&original.path(), link, &options)?;
            } else {
                symlink_or_descend(
                    &original.path(),
                    &PathBuf::from_iter([
                        Path::new("/"),
                        original.path().strip_prefix(&package_path)?,
                    ]),
                    &options,
                )?;
            }
        }
    }

    Ok(())
}

/// Returns a substring with the `U+0040 AT SIGN (@)` prefix removed.
///
/// If the string starts with a `U+0040 AT SIGN (@)`, returns substring after the prefix, wrapped
/// in `Some`. The prefix is removed exactly once.
///
/// If the string does not start with a `U+0040 AT SIGN (@)`, returns `None`.
fn strip_at_sign_prefix(file_name: &OsStr) -> Option<&OsStr> {
    let file_name = file_name.as_bytes();

    if file_name[0] == b'@' {
        Some(OsStr::from_bytes(&file_name[1..]))
    } else {
        None
    }
}

/// Symlink the children of `original` to the children of `link`.
fn descend_and_symlink(original: &Path, link: &Path, options: &Options) -> Result<()> {
    for entry in original
        .read_dir()
        .with_context(|| format!("Unable to descend into {}", original.display()))?
    {
        let entry = entry?;

        symlink_or_descend(&entry.path(), &link.join(entry.file_name()), options)?;
    }

    Ok(())
}

/// Symlink `original` to `link`, or, if `original` already exists and is a directory, calls [`descend_and_symlink`].
fn symlink_or_descend(original: &Path, link: &Path, options: &Options) -> Result<()> {
    match (link.metadata(), original.metadata()) {
        (Ok(a), Ok(b)) if a.ino() == b.ino() && a.dev() == b.dev() => {
            if options.unlink {
                println!("Removing symlink: {}", link.display());

                if !options.dry_run {
                    std::fs::remove_file(link).context("Unable to remove symlink")?;
                }
            } else if options.verbosity > 0 {
                println!("Skipping preexisting symlink: {}", link.display());
            }

            Ok(())
        }
        (Ok(link_metadata), _) => {
            if link_metadata.is_file() {
                bail!("{} already exists", link.display());
            }

            if options.verbosity > 0 {
                println!("Descending into preexisting directory: {}", link.display());
            }

            descend_and_symlink(original, link, options)?;

            Ok(())
        }
        _ => {
            if !options.unlink {
                println!("{} => {}", link.display(), original.display());

                if !options.dry_run {
                    symlink(original, link).with_context(|| {
                        format!(
                            "Unable to symlink {} => {}",
                            link.display(),
                            original.display()
                        )
                    })?;
                }
            } else if options.verbosity > 0 {
                println!("Skipping non-existent file: {}", link.display());
            }

            Ok(())
        }
    }
}
