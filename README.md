# nixpick

TUI for picking nixpkgs packages into a direnv-managed flake.

Search [search.nixos.org](https://search.nixos.org) from the terminal, multi-select packages into a basket, then generate a project `flake.nix` + `.envrc` and run `direnv allow`.

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

## Usage

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

### Output

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
