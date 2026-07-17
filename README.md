# nixpick

Pick nixpkgs packages into a direnv-managed flake.

Search [search.nixos.org](https://search.nixos.org) from the terminal, multi-select packages into a basket, then generate a project `flake.nix` + `.envrc` and run `direnv allow`.

nixpick has two modes:

- **TUI** — interactive terminal UI for browsing and multi-selecting packages.
- **CLI** — scriptable subcommands for agents and shell scripts.

## Install

```bash
cd ~/Projects/nixpick
cargo build --release
# optional: put it on PATH
cp target/release/nixpick ~/.local/bin/
```

Or run without installing:

```bash
cargo run --release -- /path/to/project
```

## TUI

```bash
cd my-project
nixpick          # target = cwd
nixpick ./app    # target = ./app
```

### Keys

| Key | Action |
|-----|--------|
| type | search (debounced ~280ms) |
| `Enter` | search immediately |
| `j`/`k` or arrows | move cursor |
| `Tab` | cycle focus: search → results → basket |
| `Space` / `Enter` | add/remove package from basket |
| `g` | generate `flake.nix` + `.envrc` + `direnv allow` |
| `G` | force overwrite existing files |
| `c` | cycle channel (unstable → 25.11 → 24.11) |
| `/` | focus search |
| `?` | help |
| `q` / `Esc` | quit |

## CLI

Run with no subcommand to launch the TUI. Use subcommands for non-interactive use.

```bash
nixpick --help
```

```
nixpick picks nixpkgs packages into a direnv-managed flake.
Run with no subcommand to launch the interactive TUI.

CLI workflow:
  nixpick add ripgrep        # search & stage the best match
  nixpick add fd bat         # stage multiple by top match each
  nixpick list               # show staged packages
  nixpick remove fd          # unstage a package
  nixpick generate           # write flake.nix + .envrc from basket

Or skip staging: `nixpick generate ripgrep fd` writes directly.

Usage: nixpick [OPTIONS] [COMMAND]

Commands:
  search    Search nixpkgs and print matching packages
  add       Search for each query and stage the best match into the basket
  list      Print the packages currently staged in the basket
  remove    Remove a package from the basket by attribute name
  generate  Generate a flake.nix + .envrc from the basket, or from explicit attrs
  help      Print this message or the help of the given subcommand(s)

Options:
      --target <TARGET>
          Target directory for the generated flake (default: current directory)
      --channel <CHANNEL>
          Nixpkgs channel: unstable (default), 25.11, 24.11
  -h, --help
          Print help (see a summary with '-h')
  -V, --version
          Print version
```

### Global options

These work on every subcommand.

| Option | Description |
|--------|-------------|
| `--target <DIR>` | Target directory for the generated flake (default: cwd) |
| `--channel <CHAN>` | Nixpkgs channel: `unstable` (default), `25.11`, `24.11` |

### `search` — query nixpkgs and print results

```bash
nixpick search ripgrep
nixpick search "python requests" --limit 10
nixpick search ripgrep --channel 25.11
```

```
Usage: nixpick search [OPTIONS] [QUERY]...

Arguments:
  [QUERY]...  Search query (e.g. "ripgrep", "python requests")

Options:
      --limit <LIMIT>  Maximum number of results to print [default: 30]
```

Prints a numbered table of matches (attr name, version, description). Does not modify anything.

### `add` — search and stage the best match

```bash
nixpick add ripgrep
nixpick add node ripgrep fd
nixpick add node --channel 25.11
```

```
Usage: nixpick add [OPTIONS] <QUERIES>...

Arguments:
  <QUERIES>...  One or more search queries; each resolves to its top match
```

For each query, searches nixpkgs, picks the **top result**, and appends it to `.nixpick-basket` in the target dir. Re-adding the same attr is a no-op (prints "Already in basket: skipped"). This is the primary staging step before `generate`.

> **Note:** `add` picks the top Elasticsearch match, which is sometimes surprising (e.g. `add fd` → `busybox`). Verify with `list` before `generate`, or use `nixpick generate <exact-attr>` to bypass fuzzy matching.

### `list` — show staged packages

```bash
nixpick list
```

Prints the attrs currently in `.nixpick-basket`. Empty basket prints a hint.

### `remove` — unstage a package

```bash
nixpick remove busybox
```

```
Usage: nixpick remove [OPTIONS] <ATTR>

Arguments:
  <ATTR>  Attribute name to unstage (e.g. nodejs_22)
```

Removes by exact attr name (as shown by `list`). No-op if the attr isn't staged.

### `generate` — write the flake

```bash
nixpick generate                 # from .nixpick-basket
nixpick generate ripgrep fd      # from explicit attrs (skips basket)
nixpick generate --force         # overwrite existing flake.nix / .envrc
```

```
Usage: nixpick generate [OPTIONS] [ATTRS]...

Arguments:
  [ATTRS]...  Package attributes to write directly (skips the basket)

Options:
  -f, --force  Force overwrite of an existing flake.nix / .envrc
```

With no attrs, reads staged packages from `.nixpick-basket` (bails if empty). With attrs, writes them directly. After a successful generate from the basket, the `.nixpick-basket` file is deleted.

### Typical agent flow

```bash
mkdir my-project && cd my-project
nixpick add ripgrep fd bat
nixpick list
nixpick generate
```

All commands exit non-zero on error with a clear message, so they chain with `&&` and are safe to script.

## Output

```
flake.nix   # multi-system devShell with selected packages
.envrc      # use flake
```

Then `direnv allow` is run automatically if `direnv` is on `PATH`.

## How it fits your setup

- **Global tools** (node, bun, npm, rust, …) stay in home-manager — available everywhere.
- **Project tools** go through nixpick → flake + direnv — layered only inside the project.
- Leaving the directory unloads the project shell; global tools remain.

## Notes

- Search hits the public search.nixos.org Elasticsearch backend (same API the website uses). Needs network.
- Schema version is auto-probed and cached under `~/.cache/nixpick/schema` when the index bumps.
- Nested attrs like `python3Packages.requests` work — written as-is into the flake.
- CLI staging state lives in `.nixpick-basket` in the target dir; it's removed after a successful `generate`.
