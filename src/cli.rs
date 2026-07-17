use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::generate;
use crate::search::{Package, SearchClient};

const BASKET_FILENAME: &str = ".nixpick-basket";

#[derive(Debug, Parser)]
#[command(
    name = "nixpick",
    version,
    about = "Pick nixpkgs packages into a direnv-managed flake",
    long_about = "nixpick picks nixpkgs packages into a direnv-managed flake.\n\
                   Run with no subcommand to launch the interactive TUI.\n\n\
                   CLI workflow:\n  \
                   nixpick add ripgrep        # search & stage the best match\n  \
                   nixpick add fd bat         # stage multiple by top match each\n  \
                   nixpick list               # show staged packages\n  \
                   nixpick remove fd          # unstage a package\n  \
                   nixpick generate           # write flake.nix + .envrc from basket\n\n\
                   Or skip staging: `nixpick generate ripgrep fd` writes directly."
)]
pub struct Cli {
    /// Target directory for the generated flake (default: current directory).
    #[arg(long, global = true)]
    pub target: Option<PathBuf>,

    /// Nixpkgs channel: unstable (default), 25.11, 24.11.
    #[arg(long, global = true)]
    pub channel: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Search nixpkgs and print matching packages.
    Search {
        /// Search query (e.g. "ripgrep", "python requests").
        query: Vec<String>,

        /// Maximum number of results to print.
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },

    /// Search for each query and stage the best match into the basket.
    ///
    /// `nixpick add node` searches "node" and appends the top result
    /// (e.g. nodejs_22) to .nixpick-basket. Repeat to build up a set,
    /// then run `nixpick generate`.
    Add {
        /// One or more search queries; each resolves to its top match.
        #[arg(required = true)]
        queries: Vec<String>,
    },

    /// Print the packages currently staged in the basket.
    List,

    /// Remove a package from the basket by attribute name.
    Remove {
        /// Attribute name to unstage (e.g. nodejs_22).
        attr: String,
    },

    /// Generate a flake.nix + .envrc from the basket, or from explicit attrs.
    ///
    /// With no attrs, reads staged packages from .nixpick-basket.
    /// With attrs, writes them directly (skips the basket).
    Generate {
        /// Package attributes to write directly (skips the basket).
        attrs: Vec<String>,

        /// Force overwrite of an existing flake.nix / .envrc.
        #[arg(long, short = 'f')]
        force: bool,
    },
}

pub async fn run(cli: Cli) -> Result<()> {
    let target = cli
        .target
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    match cli.command {
        None => unreachable!("tui is dispatched before run()"),
        Some(Command::Search { query, limit }) => {
            let query = query.join(" ");
            if query.trim().is_empty() {
                bail!("search query is empty");
            }
            let mut client = SearchClient::new(cli.channel.clone());
            let (packages, elapsed, total) = client.search(&query, limit).await?;
            if packages.is_empty() {
                println!("No results for '{query}'.");
            } else {
                print_results(&packages, elapsed, total, client.channel());
            }
            Ok(())
        }
        Some(Command::Add { queries }) => {
            let channel = cli.channel.clone();
            let mut client = SearchClient::new(channel);
            let basket = basket_path(&target);

            for q in queries {
                if q.trim().is_empty() {
                    bail!("empty query in add list");
                }
                let (packages, _elapsed, total) = client.search(&q, 1).await?;
                match packages.into_iter().next() {
                    Some(pkg) => {
                        if basket_contains(&basket, &pkg.attr_name) {
                            println!("Already in basket: {} (skipped)", pkg.attr_name);
                        } else {
                            append_basket(&basket, &pkg)?;
                            println!(
                                "Added {} ({}) — {}",
                                pkg.attr_name,
                                if pkg.version.is_empty() {
                                    "?"
                                } else {
                                    &pkg.version
                                },
                                if total == 1 {
                                    "only match".to_string()
                                } else {
                                    format!("top of {total} matches for '{q}'")
                                }
                            );
                        }
                    }
                    None => println!("No match for '{q}' — skipped"),
                }
            }
            Ok(())
        }
        Some(Command::List) => {
            let basket = basket_path(&target);
            let pkgs = read_basket(&basket);
            if pkgs.is_empty() {
                println!("Basket is empty. Use `nixpick add <query>` to stage packages.");
            } else {
                println!("Basket ({}):", pkgs.len());
                for p in &pkgs {
                    let v = if p.version.is_empty() {
                        ""
                    } else {
                        &p.version
                    };
                    println!("  • {:<32} {}", p.attr_name, v);
                }
            }
            Ok(())
        }
        Some(Command::Remove { attr }) => {
            let basket = basket_path(&target);
            let mut pkgs = read_basket(&basket);
            let before = pkgs.len();
            pkgs.retain(|p| p.attr_name != attr);
            if pkgs.len() == before {
                println!("{} not in basket.", attr);
            } else {
                write_basket(&basket, &pkgs)?;
                println!("Removed {} from basket ({} remain).", attr, pkgs.len());
            }
            Ok(())
        }
        Some(Command::Generate { attrs, force }) => {
            let channel = cli.channel.clone();
            let client = SearchClient::new(channel);

            let packages = if attrs.is_empty() {
                let pkgs = read_basket(&basket_path(&target));
                if pkgs.is_empty() {
                    bail!(
                        "basket is empty — use `nixpick add <query>` first, or pass attrs: \
                         `nixpick generate ripgrep fd`"
                    );
                }
                pkgs
            } else {
                attrs
                    .into_iter()
                    .map(|attr| Package {
                        attr_name: attr,
                        version: String::new(),
                        description: String::new(),
                    })
                    .collect()
            };

            let result = generate::generate(&target, &packages, client.channel(), force)?;
            print_generate_result(&result);

            // Clean up the staging basket after a successful generate from it.
            if !result.packages.is_empty() {
                let basket = basket_path(&target);
                if basket.exists() {
                    let _ = std::fs::remove_file(&basket);
                }
            }
            Ok(())
        }
    }
}

fn basket_path(target: &Path) -> PathBuf {
    target.join(BASKET_FILENAME)
}

fn basket_contains(basket: &Path, attr: &str) -> bool {
    read_basket(basket).iter().any(|p| p.attr_name == attr)
}

fn read_basket(basket: &Path) -> Vec<Package> {
    let Ok(text) = std::fs::read_to_string(basket) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut parts = l.splitn(2, '\t');
            let attr = parts.next().unwrap_or(l).trim().to_string();
            let version = parts.next().unwrap_or("").trim().to_string();
            Package {
                attr_name: attr,
                version,
                description: String::new(),
            }
        })
        .collect()
}

fn append_basket(basket: &Path, pkg: &Package) -> Result<()> {
    if let Some(parent) = basket.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    let line = if pkg.version.is_empty() {
        format!("{}\n", pkg.attr_name)
    } else {
        format!("{}\t{}\n", pkg.attr_name, pkg.version)
    };
    let mut content = std::fs::read_to_string(basket).unwrap_or_default();
    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str(&line);
    std::fs::write(basket, content)
        .with_context(|| format!("write {}", basket.display()))?;
    Ok(())
}

fn write_basket(basket: &Path, pkgs: &[Package]) -> Result<()> {
    if pkgs.is_empty() {
        if basket.exists() {
            let _ = std::fs::remove_file(basket);
        }
        return Ok(());
    }
    let mut content = String::new();
    for p in pkgs {
        if p.version.is_empty() {
            content.push_str(&format!("{}\n", p.attr_name));
        } else {
            content.push_str(&format!("{}\t{}\n", p.attr_name, p.version));
        }
    }
    std::fs::write(basket, content).with_context(|| format!("write {}", basket.display()))?;
    Ok(())
}

fn print_results(packages: &[Package], elapsed: std::time::Duration, total: u64, channel: &str) {
    println!(
        "Found {} of {} results in {}ms (channel: {})\n",
        packages.len(),
        total,
        elapsed.as_millis(),
        channel
    );
    for (i, p) in packages.iter().enumerate() {
        let desc = if p.description.is_empty() {
            ""
        } else {
            &p.description
        };
        let version = if p.version.is_empty() {
            "-"
        } else {
            &p.version
        };
        println!("{:>3}. {:<32} {:<12} {}", i + 1, p.attr_name, version, desc);
    }
}

fn print_generate_result(r: &generate::GenerateResult) {
    println!(
        "Wrote {} packages to {} · {}",
        r.packages.len(),
        r.flake_path.display(),
        if r.direnv_allowed {
            "direnv allow ✓"
        } else {
            r.direnv_message.as_str()
        }
    );
}
