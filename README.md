# Shim

Create shims for executables from config files.

## Stability Warning

API may change wildly until while the major version number is 0.
Once this crate's version hits `1.0.0` api changes will only happen with major version bumps.

## Installation

### With Cargo

```bash
cargo install shim
```

## Usage

### Cli

```bash
shim --help
```

### Shim config files

## Motivation

### Condensing configuration

My git configuration was scattered over too many places

- Git allows arbitrary commands to be named with prefix `git-` and git will

- My git config file defined aliases

```toml
[alias]
url = remote get-url origin
root = rev-parse --show-toplevel
commit = !cz commit
cz = !cz commit
```

- My bashrc has a function that
  - adds pre/post hooks globally, for all git commands
  - overrides some git subcommands

```bash
function git() {
  # Run pre-commit first for immediate feedback
  pre-commit # TODO: only run on some git commands

  # Override functionality based on the first argument, the subcommmand
  case "$1" in
    'commit')
      cz commit
      ;;
    'init'|clone)
      # Run command before creating a new repo on filesystem
      git-track-repos --quiet "$@"
      # Unlike other commands, make sure to run the git operation still
      git "$@"
      ;;
    *)
      # Fall back to actual git for everything else
      git "${@}"
      ;;
  esac

  # Run these after any git operation completes
  git-track-repos # This is a program I wrote to keep track of all repos I've cloned
}
```

- Git hooks

### Forking fewer times when calling git

I've created quite a few hooks.
I've written a tool for some of my hooks (TODO: publish that tool and link here) in rust. I plan for `shim` to be able to load shims from dynamic libraries instead of forking a bunch of processes
