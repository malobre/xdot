# xdot

A minimalist dotfiles manager inspired by a
[2012 blog post by Brandon Invergo][1] ([archived][2]) in which they show how to
use GNU stow to manage dotfiles. I found this solution very elegant, however if
you happen to use different path for, e.g, `$XDG_CONFIG_HOME` between your
computers, this won't work. `xdot` is my answer to that.

## Usage

`xdot [PACKAGE ...]` will symlink the content of specified packages to the
appropriate locations. This action is idempotent and will NEVER overwrite
existing files, if a directory already exists it will descend into it until it
is able to symlink.

## Directory Structure

```
~
└── .xdot
    ├── PACKAGE_0
    ├── PACKAGE_1
    ├── ...
    └── PACKAGE_N
```

## Packages

A package is a directory that contains config for an application.

If a subdirectory of a package begins with a `U+0040 AT SIGN (@)`, the remaining
characters will be interpreted as an environment variable name (with spec
compliant defaults for XDG Base Directory vars), e.g:

- `PACKAGE/@HOME/FILE` will be symlinked to `$HOME/FILE`
- `PACKAGE/@XDG_CONFIG_HOME/FILE` will be symlinked `$XDG_CONFIG_HOME/FILE`

[1]: http://brandon.invergo.net/news/2012-05-26-using-gnu-stow-to-manage-your-dotfiles.html
[2]: https://web.archive.org/web/20220617221459/http://brandon.invergo.net/news/2012-05-26-using-gnu-stow-to-manage-your-dotfiles.html
[3]: https://specifications.freedesktop.org/basedir-spec/0.8/
