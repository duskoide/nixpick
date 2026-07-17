use std::path::PathBuf;
use std::time::Duration;

use crate::search::Package;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Search,
    Results,
    Basket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Ok,
    Error,
    Searching,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub kind: StatusKind,
    pub message: String,
}

pub struct App {
    pub query: String,
    pub results: Vec<Package>,
    pub result_cursor: usize,
    pub basket: Vec<Package>,
    pub basket_cursor: usize,
    pub focus: Focus,
    pub channel: String,
    pub total_hits: u64,
    pub last_search_ms: u64,
    pub status: Status,
    pub show_help: bool,
    pub target_dir: PathBuf,
    pub should_quit: bool,
    pub pending_query: Option<String>,
    pub search_generation: u64,
}

impl App {
    pub fn new(target_dir: PathBuf, channel: String) -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            result_cursor: 0,
            basket: Vec::new(),
            basket_cursor: 0,
            focus: Focus::Search,
            channel,
            total_hits: 0,
            last_search_ms: 0,
            status: Status {
                kind: StatusKind::Info,
                message: "Type to search nixpkgs · Space: toggle basket · g: generate".into(),
            },
            show_help: false,
            target_dir,
            should_quit: false,
            pending_query: None,
            search_generation: 0,
        }
    }

    pub fn set_searching(&mut self) {
        self.status = Status {
            kind: StatusKind::Searching,
            message: format!("Searching '{}'…", self.query),
        };
    }

    pub fn apply_results(
        &mut self,
        packages: Vec<Package>,
        elapsed: Duration,
        total: u64,
        generation: u64,
    ) {
        if generation != self.search_generation {
            return;
        }
        self.results = packages;
        self.total_hits = total;
        self.last_search_ms = elapsed.as_millis() as u64;
        if self.result_cursor >= self.results.len() {
            self.result_cursor = self.results.len().saturating_sub(1);
        }
        self.status = Status {
            kind: StatusKind::Info,
            message: format!(
                "{} of {} results · {}ms · channel: {}",
                self.results.len(),
                self.total_hits,
                self.last_search_ms,
                self.channel
            ),
        };
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.status = Status {
            kind: StatusKind::Error,
            message: msg.into(),
        };
    }

    pub fn set_ok(&mut self, msg: impl Into<String>) {
        self.status = Status {
            kind: StatusKind::Ok,
            message: msg.into(),
        };
    }

    pub fn request_search(&mut self) {
        self.search_generation = self.search_generation.wrapping_add(1);
        self.pending_query = Some(self.query.clone());
        if self.query.trim().is_empty() {
            self.results.clear();
            self.total_hits = 0;
            self.status = Status {
                kind: StatusKind::Info,
                message: "Type to search nixpkgs · Space: toggle basket · g: generate".into(),
            };
        } else {
            self.set_searching();
        }
    }

    pub fn move_cursor(&mut self, delta: isize) {
        match self.focus {
            Focus::Results if !self.results.is_empty() => {
                let len = self.results.len() as isize;
                let next = (self.result_cursor as isize + delta).rem_euclid(len);
                self.result_cursor = next as usize;
            }
            Focus::Basket if !self.basket.is_empty() => {
                let len = self.basket.len() as isize;
                let next = (self.basket_cursor as isize + delta).rem_euclid(len);
                self.basket_cursor = next as usize;
            }
            _ => {}
        }
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Search => {
                if !self.results.is_empty() {
                    Focus::Results
                } else if !self.basket.is_empty() {
                    Focus::Basket
                } else {
                    Focus::Search
                }
            }
            Focus::Results => {
                if !self.basket.is_empty() {
                    Focus::Basket
                } else {
                    Focus::Search
                }
            }
            Focus::Basket => Focus::Search,
        };
    }

    pub fn toggle_selected(&mut self) {
        match self.focus {
            Focus::Results => {
                if let Some(pkg) = self.results.get(self.result_cursor).cloned() {
                    if let Some(idx) = self.basket.iter().position(|p| p.attr_name == pkg.attr_name)
                    {
                        self.basket.remove(idx);
                        if self.basket_cursor >= self.basket.len() {
                            self.basket_cursor = self.basket.len().saturating_sub(1);
                        }
                        self.set_ok(format!("Removed {}", pkg.attr_name));
                    } else {
                        self.basket.push(pkg.clone());
                        self.set_ok(format!("Added {}", pkg.attr_name));
                    }
                }
            }
            Focus::Basket => {
                if self.basket_cursor < self.basket.len() {
                    let name = self.basket[self.basket_cursor].attr_name.clone();
                    self.basket.remove(self.basket_cursor);
                    if self.basket_cursor >= self.basket.len() {
                        self.basket_cursor = self.basket.len().saturating_sub(1);
                    }
                    self.set_ok(format!("Removed {name}"));
                }
            }
            Focus::Search => {}
        }
    }

    pub fn is_in_basket(&self, attr: &str) -> bool {
        self.basket.iter().any(|p| p.attr_name == attr)
    }
}

