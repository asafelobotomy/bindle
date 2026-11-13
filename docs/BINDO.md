# bindo: Bindle's recipe runner

`bindo` runs *recipes*: short, named, documented system tasks, in the spirit
of Universal Blue's `ujust`.

```bash
bindo                       # list the recipes by category
bindo clean-journal --help  # what a recipe does, its arguments, its script
bindo clean-journal 1G      # run it
bindo -n orphans            # print the command, run nothing
bindo completions bash      # tab completion for recipe names and choices
```

The listing marks recipes that run as root with `[admin]`, and recipes whose
commands aren't installed with `(needs …)`. `--help` on a recipe prints the
script it runs, so nothing runs unseen.

## Design

### Recipe source: Bindle's own TOML, not justfiles

`ujust` wraps `just`. bindo uses its own small format instead, kept in TOML:

- **No new parser and no new dependency.** The `toml` crate is already in the
  workspace (the catalog uses it); `serde` with `deny_unknown_fields` does the
  parsing and the checks are plain Rust with unit tests. `just` would be one
  more package on every install, and it isn't in the AppImage.
- **Structured metadata.** Category, one-line description, longer help,
  `admin`, typed arguments with defaults and choices, required commands and a
  confirmation question are fields, not conventions in comments. That is what
  the grouped listing, `[admin]` marker, `--help` and completion are built from.
- **Arguments never become script text.** `just` pastes `{{arg}}` into the
  recipe. bindo passes values as `$1`, `$2`, … to `sh -ec <script>`, and every
  value is checked against its kind or choices before anything runs. No value
  may start with `-`, so none can turn into an option of the commands it
  reaches.
- **The cost:** no `just` features (dependencies between recipes, variables,
  other interpreters). A recipe that needs those can `exec` a script or `just`.

### Format

```toml
[[recipe]]
name = "clean-journal"            # lowercase, digits, `-`; up to 32
category = "Maintenance"          # System, Maintenance, Setup, Toggles, or your own
description = "Shrink the system journal to a size"   # one line, ≤ 72 characters
help = "Longer text for --help."  # optional
admin = true                      # runs as root, after a polkit password prompt
requires = ["journalctl"]         # commands it needs on PATH
confirm = "Delete the old logs?"  # asked (y/N) first; --yes skips it
args = [
    { name = "size", kind = "size", default = "500M", help = "Size to keep." },
]
run = 'journalctl --vacuum-size="$1"'   # sh -e; arguments are $1, $2, …
```

Argument fields: `name`, `help`, `default` (makes it optional), `choices`
(the only values accepted; offered by completion), `kind` (`word`, the
default: letters, digits and `._+-:@`; `number`; `size`, digits with an
optional K/M/G; `text`, any one line) and `rest` (last argument only: takes
every remaining value). Required arguments come before optional ones.

### Where recipes live

Loaded in this order; a recipe with the name of an earlier one replaces it:

1. Shipped: `crates/bindo/recipes/*.toml`, compiled into the binary (a unit
   test validates every one).
1. Catalog modules: `[[module.recipe]]` tables in `catalog/arch/*.toml`,
   same format, listed only while the module is installed (all of its
   packages, AUR packages and pinned releases are there; a module that
   installs nothing never lists any). Catalog validation checks them and
   refuses a name a shipped recipe or another module already uses.
2. `/usr/share/bindle/recipes/*.toml`: added by a distribution package.
3. `/etc/bindle/recipes/*.toml`: the admin's own.
4. `~/.config/bindo/recipes/*.toml` (`$XDG_CONFIG_HOME`): the user's own.

A broken file is skipped with a warning; the rest still load.

**Admin recipes only from root-owned directories.** An `admin = true` recipe
is accepted only from a directory that, with every parent, is owned by root
and not writable by others. A user directory can add plain recipes and
override any recipe with a plain one, but can't slip a different script in
behind a password prompt the user expects from a shipped recipe.

### Root: polkit, never sudo

An admin recipe runs as `pkexec /bin/sh -ec <script> bindo-<name> <args>`:
the desktop's password dialog, or pkexec's text prompt on a console. Without
pkexec, `run0` (systemd's polkit-based equivalent). Already root: the script
runs directly. The terminal stays attached, so `pacman` or `pacdiff` inside a
recipe can ask their own questions. pkexec clears the environment; a recipe
that needs a variable sets it itself.

Every approval in Bindle goes through pkexec (`bindle apply`, `undo` and
`bindleupgrade` start their root helper with it too).

bindo doesn't use Bindle's root helper here: the helper runs one command at a
time with no terminal, which would make every interactive recipe impossible.
`bindleupgrade` does use it (one prompt for many root steps).

### One binary, three names

bindo is a library crate (`crates/bindo`: format, loading, argument checks,
listing, completion scripts, execution) and a subcommand of the `bindle`
binary (`crates/bindle-cli/src/bindo_cmd.rs`). `bindle first-login install`
links `/usr/local/bin/bindo` and `/usr/local/bin/bindleupgrade` to
`/usr/local/bin/bindle`; the binary looks at the name it was started as. An
AppImage works the same when linked or renamed to `bindo` (its runtime sets
`ARGV0`). `bindle bindo …` works everywhere.

## Shipped recipes

| Category | Recipes |
|---|---|
| System | `info`, `kernel`, `errors [lines]`, `failed`, `logs <unit> [lines]` |
| Maintenance | `update [all\|check]`, `update-notify on\|off`, `upgrade-prefs`, `clean-pkg-cache [keep]` [admin], `clean-journal [size]` [admin], `clean-thumbnails`, `orphans` [admin], `pacnew` [admin], `mirrors` [admin], `fix-keyring` [admin] |
| Setup | `setup`, `whats-new`, `add <module>`, `printer`, `undo`, `completions <shell>` |
| Toggles | `bluetooth on\|off` [admin], `ssh on\|off` [admin], `wifi on\|off` |

From catalog modules, when installed:

| Module | Recipes |
|---|---|
| `snapper` | `snapshots` [admin], `snapshot [note]` [admin] |
| `ufw` | `firewall on\|off\|status` [admin] |
| `power-profiles` | `power-profile <profile>` |
| `printing` | `printers`, `printing-restart` [admin] |
| `flatpak` | `flatpak-clean` |

Re-running part of Bindle's setup goes through `bindle apply --with <module>`
(`add`, `printer`), so it shows the plan, snapshots and can be undone like the
wizard.

## Not done (choices left open)

- Hidden or private recipes (`_name`), recipe aliases, and grouping
  sub-recipes (`bindo toggle bluetooth on`) are not supported.
- zsh and fish completion scripts are written but untested here (neither
  shell is installed on the dev machine); bash completion is tested.
