# xdot

A minimalist dotfiles manager inspired by [a 2012 blog post][1] ([archived][2])
by Brandon Invergo in which they show how to use GNU stow to manage dotfiles. I
found this solution very elegant, however if you happen to use different path
for, e.g, `$XDG_CONFIG_HOME` between your computers, this won't work. `xdot` is
my answer to that.

## Packages

A package is a directory that contains config for an application.

`xdot` will look for packages in `~/.xdot`.

If a package subdirectory's name begins with a `U+0040 AT SIGN (@)`, the
remaining characters will be interpreted as an environment variable name (with
spec compliant defaults for XDG Base Directory vars), e.g:

- `PACKAGE/@HOME/FILE` will be symlinked to `$HOME/FILE`,
- `PACKAGE/@XDG_CONFIG_HOME/FILE` will be symlinked to `$XDG_CONFIG_HOME/FILE`.

Otherwise, xdot will link the content of said package relative to `/`, e.g:

- `PACKAGE/FILE` will be symlinked to `/FILE`,
- `PACKAGE/DIR/FILE` will be symlinked to `/DIR/FILE`.

## Usage

```
Usage: xdot [options] [--] package...
Symlink your dotfiles from `~/.xdot`.

Options:
  --unlink       Remove symlinks.
  --dry-run      Don't modify the file system.
  -v, --verbose  Increase verbosity.
  -h, --help     Show this help message and exit.
  --version      Show version information and exit.
```

Running `xdot` is idempotent and won't overwrite existing files, if a directory
already exists it will descend into it until it is able to symlink or fails.

`--unlink` will remove symlinks that would otherwise be created (except if the
existing link points to a location outside of `~/.xdot`).

[1]: http://brandon.invergo.net/news/2012-05-26-using-gnu-stow-to-manage-your-dotfiles.html
[2]: https://web.archive.org/web/20220617221459/http://brandon.invergo.net/news/2012-05-26-using-gnu-stow-to-manage-your-dotfiles.html
[3]: https://specifications.freedesktop.org/basedir-spec/0.8/
