---
description: Scan the project, stage matching nixpkgs packages, and generate flake.nix + .envrc
argument-hint: "[--force]"
---

You are bootstrapping a Nix flake for the current project using `nixpick`. Your
job: detect the project's dependencies, map them to nixpkgs packages, stage
them with the nixpick CLI, and generate `flake.nix` + `.envrc`.

The optional argument is `--force` — pass it through to `nixpick generate` if
the user wants to overwrite an existing `flake.nix`.

## 0. Pre-flight

- Confirm `nixpick` is on `PATH` (`which nixpick`). If not, tell the user to
  install it (`cargo install --path .` or copy `target/release/nixpick` to
  `~/.local/bin/`) and stop.
- Target directory is the current working directory.
- If `flake.nix` already exists and `--force` was not passed, STOP and ask
  the user before continuing — `nixpick generate` refuses to overwrite
  without `--force`. (You can preview the existing flake to make the prompt
  useful.)
- If `.nixpick-basket` exists from a previous run, mention it; we will
  replace its contents.

## 1. Scan for project markers

Look for these files in the project root (use `ls` / `find`):

| Marker | Means |
|---|---|
| `package.json`, `node_modules/`, `.nvmrc`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lockb` | Node.js |
| `requirements.txt`, `pyproject.toml`, `setup.py`, `Pipfile`, `poetry.lock`, `uv.lock`, `.python-version` | Python |
| `Cargo.toml`, `Cargo.lock` | Rust |
| `go.mod`, `go.sum` | Go |
| `Gemfile`, `Gemfile.lock`, `.ruby-version` | Ruby |
| `mix.exs`, `mix.lock` | Elixir |
| `pom.xml`, `build.gradle*` | Java |
| `composer.json` | PHP |
| `deno.json`, `deno.lock` | Deno |
| `Dockerfile`, `docker-compose.yml` | Inspect for system deps, postgres, redis, etc. |
| `Makefile`, `scripts/` | Look for shell tools invoked: ripgrep, fd, jq, bat, fzf, … |

Read the relevant files to extract concrete versions where the project
pins them (e.g. `.nvmrc` → `22`, `pyproject.toml` `requires-python` →
`>=3.12`, `.python-version` → `3.12.5`). Note any CLI tools the Makefile or
scripts invoke.

## 2. Map to nixpkgs

Translate each detected dep into a `nixpick add` query. Defaults:

| Detected | nixpick add |
|---|---|
| Node (any) | `nodejs` (or `nodejs_22` / `nodejs_20` if version is pinned) |
| Bun | `bun` |
| Deno | `deno` |
| Python 3.x | `python3` (or `python311` / `python312` / `python313` if pinned) |
| Rust toolchain | **do not stage** — system rust + rustup is faster; flag it and skip |
| Go | `go` |
| Ruby | `ruby` |
| Elixir | `elixir` |
| Java | `openjdk` (or `openjdk17` / `openjdk21` if version pinned) |
| PHP | `php` |
| Postgres | `postgresql` |
| Redis | `redis` |
| ripgrep / fd / bat / jq / … | add the tool by name directly |
| `python3Packages.foo` | `nixpick add python3Packages.foo` (stages verbatim) |

When in doubt, use the bare language/runtime name — `nixpick add` picks the
top search match, which is the most common attr.

## 3. Show the plan, then stage

Before staging, print the planned `nixpick add` commands in a single code
block so the user can see what's about to run. Then stage them:

```bash
nixpick add <query-1>
nixpick add <query-2>
# …
```

`add` is idempotent against the basket — duplicates are skipped with a
message.

## 4. Verify the basket

Run `nixpick list`. For each entry, check that the staged attr is what was
intended. Common mismatches:

- `add python3` may stage `ihaskell` (top match) instead of a python attr — use `nixpick search python3` to find the correct versioned attr (`python311`, `python312`, `python313`), or `nixpick remove ihaskell` and add the right one. If `.python-version` is pinned, add the exact match.
- `add fd` may stage `busybox` (top match) instead of `fd` → `nixpick remove busybox` and `nixpick add fd` (the second time, `fd` is the top match; if it still isn't, `nixpick search fd` to find the right attr and add it verbatim).
- `add node` may stage `nodejs_22` or `nodejs_20` depending on channel — verify the version matches what the project wants.
- For nested attrs (`python3Packages.requests`), add them by exact name:
  `nixpick add python3Packages.requests` stages verbatim regardless of search.

Tell the user if you removed/added anything during verification.

## 5. Generate

```bash
nixpick generate           # or: nixpick generate --force
```

This writes `flake.nix` and `.envrc` in the project root and runs
`direnv allow` if direnv is on `PATH`. The `.nixpick-basket` staging file
is removed automatically on success.

## 6. Confirm

Report back with:
- Path to the generated `flake.nix` and `.envrc`
- Whether `direnv allow` succeeded (or what to do manually if direnv is missing)
- The final list of staged attrs that landed in the flake

## Rules

- Never invent package names. If `nixpick search` for a query returns no
  useful match, ask the user for the correct attr.
- Never overwrite an existing `flake.nix` without `--force`. If the user
  didn't pass it, ask.
- Keep this short. Don't write a long plan, don't lecture about nix — just
  scan, stage, verify, generate, report.
