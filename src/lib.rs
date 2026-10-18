//! OPS — Open Poker Solver library: cards, ranges, hand evaluation, public
//! tree, vector CFR+ and reporting.

pub mod cards;
pub mod cfr;
pub mod eval;
pub mod hh;
pub mod json;
pub mod range;
pub mod report;
pub mod tree;

use cards::parse_board;
use cfr::{SolveOptions, Solution, Solver};
use json::Json;
use range::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use tree::TreeConfig;

#[derive(Clone, Debug)]
pub struct SolveConfig {
    pub board: String,
    pub oop_range: String,
    pub ip_range: String,
    pub pot: f64,
    pub stack: f64,
    /// Bet sizes per solved street, as pot fractions.
    pub bet_sizes: Vec<Vec<f64>>,
    pub max_raises: u32,
    /// Rake as a fraction of the pot (0.05 = 5%).
    pub rake_pct: f64,
    /// Rake cap in chips; 0 = uncapped.
    pub rake_cap: f64,
    pub runout_samples: usize,
    pub turn_samples: usize,
    pub iterations: usize,
    pub target_exploitability: f64,
    pub check_every: usize,
    pub seed: u64,
    pub max_nodes: usize,
}

impl Default for SolveConfig {
    fn default() -> Self {
        Self {
            board: "AhKd7c".into(),
            oop_range: "QQ+,AKs,AQo".into(),
            ip_range: "JJ+,AQs,KQs".into(),
            pot: 10.0,
            stack: 90.0,
            bet_sizes: vec![vec![0.75], vec![0.75], vec![0.75]],
            max_raises: 1,
            rake_pct: 0.0,
            rake_cap: 0.0,
            runout_samples: 6,
            turn_samples: 0,
            iterations: 400,
            target_exploitability: 0.0,
            check_every: 25,
            seed: 0x5eed_1234,
            max_nodes: 400_000,
        }
    }
}

impl SolveConfig {
    pub fn from_json(v: &Json) -> Result<SolveConfig, String> {
        let d = SolveConfig::default();
        let mut c = SolveConfig { ..d };
        if let Some(x) = v.get("board").and_then(|x| x.as_str()) {
            c.board = x.to_string();
        }
        if let Some(x) = v.get("oop_range").and_then(|x| x.as_str()) {
            c.oop_range = x.to_string();
        }
        if let Some(x) = v.get("ip_range").and_then(|x| x.as_str()) {
            c.ip_range = x.to_string();
        }
        if let Some(x) = v.get("pot").and_then(|x| x.as_f64()) {
            c.pot = x;
        }
        if let Some(x) = v.get("stack").and_then(|x| x.as_f64()) {
            c.stack = x;
        }
        if let Some(x) = v.get("bet_sizes") {
            if let Some(arr) = x.as_arr() {
                let mut sizes = Vec::new();
                for street in arr {
                    if let Some(inner) = street.as_arr() {
                        let vals: Vec<f64> = inner.iter().filter_map(|y| y.as_f64()).collect();
                        if !vals.is_empty() {
                            sizes.push(vals);
                        }
                    }
                }
                if !sizes.is_empty() {
                    c.bet_sizes = sizes;
                }
            }
        }
        if let Some(x) = v.get("max_raises").and_then(|x| x.as_usize()) {
            c.max_raises = x as u32;
        }
        if let Some(x) = v.get("rake_pct").and_then(|x| x.as_f64()) {
            c.rake_pct = x;
        }
        if let Some(x) = v.get("rake_cap").and_then(|x| x.as_f64()) {
            c.rake_cap = x;
        }
        if let Some(x) = v.get("runout_samples").and_then(|x| x.as_usize()) {
            c.runout_samples = x;
        }
        if let Some(x) = v.get("turn_samples").and_then(|x| x.as_usize()) {
            c.turn_samples = x;
        }
        if let Some(x) = v.get("iterations").and_then(|x| x.as_usize()) {
            c.iterations = x;
        }
        if let Some(x) = v.get("target_exploitability").and_then(|x| x.as_f64()) {
            c.target_exploitability = x;
        }
        if let Some(x) = v.get("check_every").and_then(|x| x.as_usize()) {
            c.check_every = x;
        }
        if let Some(x) = v.get("seed").and_then(|x| x.as_f64()) {
            c.seed = x as u64;
        }
        if let Some(x) = v.get("max_nodes").and_then(|x| x.as_usize()) {
            c.max_nodes = x;
        }
        Ok(c)
    }

    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("board", Json::str(self.board.clone())),
            ("oop_range", Json::str(self.oop_range.clone())),
            ("ip_range", Json::str(self.ip_range.clone())),
            ("pot", Json::num(self.pot)),
            ("stack", Json::num(self.stack)),
            (
                "bet_sizes",
                Json::Arr(
                    self.bet_sizes
                        .iter()
                        .map(|s| Json::Arr(s.iter().map(|x| Json::num(*x)).collect()))
                        .collect(),
                ),
            ),
            ("max_raises", Json::num(self.max_raises as f64)),
            ("rake_pct", Json::num(self.rake_pct)),
            ("rake_cap", Json::num(self.rake_cap)),
            ("runout_samples", Json::num(self.runout_samples as f64)),
            ("turn_samples", Json::num(self.turn_samples as f64)),
            ("iterations", Json::num(self.iterations as f64)),
        ])
    }
}

/// Builds the tree and runs the solver. `progress` is called every
/// `check_every` iterations.
pub fn solve(
    cfg: &SolveConfig,
    cancel: &AtomicBool,
    progress: impl FnMut(cfr::Progress),
) -> Result<(Solution, Solver), String> {
    let board = parse_board(&cfg.board)?;
    if board.len() < 3 || board.len() > 5 {
        return Err("board must have 3, 4 or 5 cards".into());
    }
    let oop = Range::from_str(&cfg.oop_range)?;
    let ip = Range::from_str(&cfg.ip_range)?;

    // Number of betting rounds solved from this board: flop -> 3, turn -> 2,
    // river -> 1.
    let rounds = 5 - board.len() + 1;
    let mut sizes = cfg.bet_sizes.clone();
    while sizes.len() < rounds {
        sizes.push(sizes.last().cloned().unwrap_or_else(|| vec![0.75]));
    }
    sizes.truncate(rounds);

    // A full flop solve enumerates 49 turn cards x 48 rivers per chance node,
    // which explodes the tree. Flop therefore defaults to runout sampling;
    // turn and river are exact unless the caller asks otherwise.
    // Flop solves enumerate 49 turns plus rivers for every betting branch, so
    // they default to a sampled runout set. Turn solves stay exact.
    let (turn_samples, runouts) = if board.len() == 3 {
        (
            if cfg.turn_samples == 0 { 7 } else { cfg.turn_samples },
            if cfg.runout_samples == 0 { 6 } else { cfg.runout_samples },
        )
    } else {
        (0, cfg.runout_samples)
    };
    // Chance nodes need at least six branches for the joint-probability
    // correction; fewer would bias or zero the runout payoffs.
    let runouts = if runouts > 0 && runouts < 6 { 6 } else { runouts };

    let tcfg = TreeConfig {
        board,
        pot: cfg.pot,
        stack: cfg.stack,
        bet_sizes: sizes,
        max_raises: cfg.max_raises,
        rake_pct: cfg.rake_pct,
        rake_cap: cfg.rake_cap,
        runout_samples: runouts,
        turn_samples,
        seed: cfg.seed,
        max_nodes: cfg.max_nodes,
    };
    let tree = tree::Tree::build(tcfg)?;
    let mut solver = Solver::new(tree, &oop, &ip)?;
    let opts = SolveOptions {
        iterations: cfg.iterations,
        target_exploitability: cfg.target_exploitability,
        check_every: cfg.check_every,
        verbose: false,
    };
    let sol = solver.solve(&opts, cancel, progress);
    Ok((sol, solver))
}

/// Convenience wrapper that also reports cancellation cleanly.
pub fn solve_cancellable(
    cfg: &SolveConfig,
    cancel: &AtomicBool,
    progress: impl FnMut(cfr::Progress),
) -> Result<(Solution, Solver), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    solve(cfg, cancel, progress)
}
