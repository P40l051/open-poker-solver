//! Vector-form CFR+ over the public betting tree.
//!
//! Every decision node owns a regret table and a strategy-sum table of
//! `actions * 1326` `f32` cells. One traversal updates every combo of the
//! acting player at once.
//!
//! Chance handling: a public card is dealt with weight `1/(n-2)` so each
//! combo's own reach mass stays exactly 1, and the branch values are scaled
//! by `(n-2)/(n-4)` to restore the joint (both hands) probability. Both
//! factors are constant per infoset, so regret matching is unaffected while
//! reported EVs stay in chips.

use crate::cards::{combo_cards, combo_mask, NCOMBOS};
use crate::eval;
use crate::range::Range;
use crate::tree::{board_mask, Kind, Terminal, Tree};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub struct Progress {
    pub iteration: usize,
    pub iterations: usize,
    pub exploitability: f64,
    pub ev_oop: f64,
}

pub struct SolveOptions {
    pub iterations: usize,
    /// Stop early when exploitability (chips) falls below this value; 0 disables.
    pub target_exploitability: f64,
    /// How often to run the exploitability check.
    pub check_every: usize,
    pub verbose: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            iterations: 800,
            target_exploitability: 0.0,
            check_every: 25,
            verbose: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActionReport {
    pub label: String,
    pub frequency: f64,
}

/// Edge from a decision node to the next decision node reachable through one
/// action. `chance` carries the public card dealt in between (turn/river).
#[derive(Clone, Debug)]
pub struct EdgeReport {
    pub action_index: usize,
    pub action: String,
    pub frequency: f64,
    pub target: usize,
    pub chance: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NodeReport {
    pub id: usize,
    pub parent: Option<usize>,
    pub street: u8,
    pub board: String,
    pub player: u8,
    pub pot: f64,
    pub actions: Vec<ActionReport>,
    pub edges: Vec<EdgeReport>,
    pub reach_oop: f64,
    pub reach_ip: f64,
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub iterations: usize,
    pub ev_oop: f64,
    pub ev_ip: f64,
    pub br_oop: f64,
    pub br_ip: f64,
    pub exploitability: f64,
    pub exploitability_pct_pot: f64,
    pub exploitability_history: Vec<(usize, f64)>,
    pub nodes: Vec<NodeReport>,
    pub terminals: usize,
    pub decisions: usize,
    pub chance_branches: usize,
    pub elapsed_ms: u128,
    pub pot: f64,
    pub board: Vec<u8>,
}

pub struct BoardEval {
    strength: Vec<u32>,
    order: Vec<u16>,
    group_of: Vec<u32>,
    pub groups: usize,
}

impl BoardEval {
    pub fn build(board: &[u8]) -> BoardEval {
        let mask = board_mask(board);
        let mut strength = vec![u32::MAX; NCOMBOS];
        let mut order = Vec::with_capacity(NCOMBOS);
        for idx in 0..NCOMBOS {
            if combo_mask(idx) & mask != 0 {
                continue;
            }
            let (a, b) = combo_cards(idx);
            strength[idx] = eval::eval7((a, b), board);
            order.push(idx as u16);
        }
        order.sort_unstable_by_key(|&i| strength[i as usize]);
        let mut group_of = vec![0u32; NCOMBOS];
        let mut groups = 0usize;
        let mut i = 0usize;
        while i < order.len() {
            let s = strength[order[i] as usize];
            let mut j = i;
            while j < order.len() && strength[order[j] as usize] == s {
                j += 1;
            }
            for &c in &order[i..j] {
                group_of[c as usize] = groups as u32;
            }
            groups += 1;
            i = j;
        }
        BoardEval {
            strength,
            order,
            group_of,
            groups,
        }
    }

    pub fn strength_of(&self, combo: usize) -> u32 {
        self.strength[combo]
    }
}

struct TerminalScratch {
    cur0: [f32; 52],
    cur1: [f32; 52],
    below0: [f32; 52],
    below1: [f32; 52],
    card0: [f32; 52],
    card1: [f32; 52],
    total0: f32,
    total1: f32,
    pre0: f32,
    pre1: f32,
    touched: Vec<u8>,
}

impl Default for TerminalScratch {
    fn default() -> Self {
        TerminalScratch {
            cur0: [0.0; 52],
            cur1: [0.0; 52],
            below0: [0.0; 52],
            below1: [0.0; 52],
            card0: [0.0; 52],
            card1: [0.0; 52],
            total0: 0.0,
            total1: 0.0,
            pre0: 0.0,
            pre1: 0.0,
            touched: Vec::with_capacity(64),
        }
    }
}

struct Scratch {
    r0: Vec<f32>,
    r1: Vec<f32>,
    o0: Vec<f32>,
    o1: Vec<f32>,
    /// Per-action child values for the acting player.
    av: Vec<f32>,
    sigma: Vec<f32>,
    term: TerminalScratch,
}

impl Scratch {
    fn new(max_actions: usize) -> Self {
        Scratch {
            r0: vec![0.0; NCOMBOS],
            r1: vec![0.0; NCOMBOS],
            o0: vec![0.0; NCOMBOS],
            o1: vec![0.0; NCOMBOS],
            av: vec![0.0; max_actions * NCOMBOS],
            sigma: vec![0.0; max_actions * NCOMBOS],
            term: TerminalScratch::default(),
        }
    }
}

pub struct Solver {
    pub tree: Tree,
    root0: Vec<f32>,
    root1: Vec<f32>,
    regrets: Vec<Vec<f32>>,
    strat_sum: Vec<Vec<f32>>,
    evals: Vec<BoardEval>,
    node_eval: Vec<usize>,
    max_actions: usize,
    pub history: Vec<(usize, f64)>,
}

impl Solver {
    pub fn new(tree: Tree, oop: &Range, ip: &Range) -> Result<Solver, String> {
        let mut evals = Vec::new();
        let mut eval_map: HashMap<u64, usize> = HashMap::new();
        let mut node_eval = vec![0usize; tree.nodes.len()];
        for (i, n) in tree.nodes.iter().enumerate() {
            if n.kind != Kind::Terminal || n.terminal != Some(Terminal::Showdown) {
                continue;
            }
            let key = board_mask(&n.board);
            let id = *eval_map.entry(key).or_insert_with(|| {
                evals.push(BoardEval::build(&n.board));
                evals.len() - 1
            });
            node_eval[i] = id;
        }
        let max_actions = tree
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Decision)
            .map(|n| n.actions.len())
            .max()
            .unwrap_or(1);

        let regrets = tree
            .nodes
            .iter()
            .map(|n| {
                if n.kind == Kind::Decision {
                    vec![0.0f32; n.actions.len() * NCOMBOS]
                } else {
                    Vec::new()
                }
            })
            .collect();
        let strat_sum = tree
            .nodes
            .iter()
            .map(|n| {
                if n.kind == Kind::Decision {
                    vec![0.0f32; n.actions.len() * NCOMBOS]
                } else {
                    Vec::new()
                }
            })
            .collect();

        let root0 = crate::tree::root_weights(oop, &tree.config.board);
        let root1 = crate::tree::root_weights(ip, &tree.config.board);
        if root0.iter().all(|w| *w == 0.0) || root1.iter().all(|w| *w == 0.0) {
            return Err("a range has no combo compatible with the board".into());
        }
        Ok(Solver {
            root0,
            root1,
            tree,
            regrets,
            strat_sum,
            evals,
            node_eval,
            max_actions,
            history: Vec::new(),
        })
    }

    fn scratch_pool(&self) -> Vec<Scratch> {
        (0..self.tree.max_depth + 3)
            .map(|_| Scratch::new(self.max_actions))
            .collect()
    }

    fn avg_strategy(&self, n: usize, out: &mut [f32]) {
        avg_strategy_reg(&self.strat_sum[n], self.tree.nodes[n].actions.len(), out);
    }

    pub fn solve(
        &mut self,
        opts: &SolveOptions,
        cancel: &AtomicBool,
        mut progress: impl FnMut(Progress),
    ) -> Solution {
        self.history.clear();
        let start = Instant::now();
        let mut scratch = self.scratch_pool();
        let mut sigma_buf = vec![0.0f32; self.max_actions * NCOMBOS];

        for it in 1..=opts.iterations {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            for update in 0..2u8 {
                scratch[0].r0.copy_from_slice(&self.root0);
                scratch[0].r1.copy_from_slice(&self.root1);
                cfr_iteration(
                    &self.tree,
                    &self.evals,
                    &self.node_eval,
                    &mut self.regrets,
                    &mut self.strat_sum,
                    &mut scratch,
                    &mut sigma_buf,
                    0,
                    it as f32,
                    update,
                );
            }

            if it % opts.check_every.max(1) == 0 || it == opts.iterations {
                let (ev0, ev1, br0, br1) = self.evaluate(&mut scratch, &mut sigma_buf);
                let expl = (((br0 - ev0) + (br1 - ev1)) / 2.0).max(0.0);
                self.history.push((it, expl));
                if opts.verbose {
                    eprintln!(
                        "iter {it:>5}  ev_oop {:+.4}  expl {:.5} ({:.2}% pot)",
                        ev0,
                        expl,
                        expl / self.tree.config.pot * 100.0
                    );
                }
                progress(Progress {
                    iteration: it,
                    iterations: opts.iterations,
                    exploitability: expl,
                    ev_oop: ev0,
                });
                if opts.target_exploitability > 0.0 && expl <= opts.target_exploitability {
                    break;
                }
            }
        }

        let (ev0, ev1, br0, br1) = self.evaluate(&mut scratch, &mut sigma_buf);
        let expl = (((br0 - ev0) + (br1 - ev1)) / 2.0).max(0.0);
        let _ = (br0, br1);
        let iterations = self.history.last().map(|x| x.0).unwrap_or(opts.iterations);
        let nodes = self.collect_nodes(&mut scratch, &mut sigma_buf);

        Solution {
            iterations,
            ev_oop: ev0,
            ev_ip: ev1,
            br_oop: br0,
            br_ip: br1,
            exploitability: expl,
            exploitability_pct_pot: expl / self.tree.config.pot * 100.0,
            exploitability_history: self.history.clone(),
            nodes,
            terminals: self.tree.terminal_count(),
            decisions: self.tree.decision_count,
            chance_branches: self
                .tree
                .nodes
                .iter()
                .filter(|n| n.kind == Kind::Chance)
                .map(|n| n.children.len())
                .sum(),
            elapsed_ms: start.elapsed().as_millis(),
            pot: self.tree.config.pot,
            board: self.tree.config.board.clone(),
        }
    }

    fn evaluate(&self, scratch: &mut [Scratch], sigma_buf: &mut [f32]) -> (f64, f64, f64, f64) {
        reset_scratch(scratch, &self.root0, &self.root1);
        avg_traverse(
            &self.tree, &self.evals, &self.node_eval, &self.strat_sum, scratch, sigma_buf, 0, None,
        );
        let ev0 = dot(&self.root0, &scratch[0].o0);
        let ev1 = dot(&self.root1, &scratch[0].o1);

        reset_scratch(scratch, &self.root0, &self.root1);
        avg_traverse(
            &self.tree, &self.evals, &self.node_eval, &self.strat_sum, scratch, sigma_buf, 0, Some(0),
        );
        let br0 = dot(&self.root0, &scratch[0].o0);

        reset_scratch(scratch, &self.root0, &self.root1);
        avg_traverse(
            &self.tree, &self.evals, &self.node_eval, &self.strat_sum, scratch, sigma_buf, 0, Some(1),
        );
        let br1 = dot(&self.root1, &scratch[0].o1);

        (ev0, ev1, br0, br1)
    }

    fn collect_nodes(&self, scratch: &mut [Scratch], sigma_buf: &mut [f32]) -> Vec<NodeReport> {
        let mut out: Vec<Option<NodeReport>> = (0..self.tree.nodes.len()).map(|_| None).collect();
        let mut parent: Vec<Option<usize>> = vec![None; self.tree.nodes.len()];
        reset_scratch(scratch, &self.root0, &self.root1);
        collect_rec(
            self, 0, scratch, sigma_buf, &mut parent, &mut out,
        );
        out.into_iter().flatten().collect()
    }

    /// Per-combo average strategy for one decision node.
    pub fn combo_strategy(&self, node: usize) -> Vec<(String, f64, Vec<f64>)> {
        let n = &self.tree.nodes[node];
        if n.kind != Kind::Decision {
            return Vec::new();
        }
        let mut sigma = vec![0.0f32; self.max_actions * NCOMBOS];
        self.avg_strategy(node, &mut sigma);
        let na = n.actions.len();
        let actor = n.player as usize;
        let root = if actor == 0 { &self.root0 } else { &self.root1 };
        let mut out = Vec::new();
        for c in 0..NCOMBOS {
            if root[c] <= 0.0 || combo_mask(c) & board_mask(&n.board) != 0 {
                continue;
            }
            let probs = (0..na)
                .map(|a| sigma[a * NCOMBOS + c] as f64)
                .collect::<Vec<_>>();
            out.push((crate::cards::combo_str(c), root[c] as f64, probs));
        }
        out
    }

    pub fn board_eval(&self, node: usize) -> Option<&BoardEval> {
        if self.tree.nodes[node].kind != Kind::Terminal {
            return None;
        }
        self.evals.get(self.node_eval[node])
    }
}

fn reset_scratch(scratch: &mut [Scratch], r0: &[f32], r1: &[f32]) {
    for s in scratch.iter_mut() {
        s.r0.fill(0.0);
        s.r1.fill(0.0);
        s.o0.fill(0.0);
        s.o1.fill(0.0);
    }
    scratch[0].r0.copy_from_slice(r0);
    scratch[0].r1.copy_from_slice(r1);
}

#[inline]
fn dot(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum()
}

#[inline]
fn avg_strategy_reg(st: &[f32], na: usize, out: &mut [f32]) {
    for c in 0..NCOMBOS {
        let mut sum = 0.0f32;
        for a in 0..na {
            sum += st[a * NCOMBOS + c];
        }
        if sum > 1e-12 {
            let inv = 1.0 / sum;
            for a in 0..na {
                out[a * NCOMBOS + c] = st[a * NCOMBOS + c] * inv;
            }
        } else {
            let u = 1.0 / na as f32;
            for a in 0..na {
                out[a * NCOMBOS + c] = u;
            }
        }
    }
}

#[inline]
fn regret_matching(reg: &[f32], na: usize, out: &mut [f32]) {
    for c in 0..NCOMBOS {
        let mut sum = 0.0f32;
        for a in 0..na {
            let r = reg[a * NCOMBOS + c];
            if r > 0.0 {
                sum += r;
            }
        }
        if sum > 0.0 {
            let inv = 1.0 / sum;
            for a in 0..na {
                let r = reg[a * NCOMBOS + c];
                out[a * NCOMBOS + c] = if r > 0.0 { r * inv } else { 0.0 };
            }
        } else {
            let u = 1.0 / na as f32;
            for a in 0..na {
                out[a * NCOMBOS + c] = u;
            }
        }
    }
}

fn copy_masked(src: &[f32], dst: &mut [f32], scale: f32, bit: u64) {
    for c in 0..NCOMBOS {
        dst[c] = if combo_mask(c) & bit == 0 {
            src[c] * scale
        } else {
            0.0
        };
    }
}

fn add_scaled(dst: &mut [f32], src: &[f32], scale: f32) {
    for c in 0..NCOMBOS {
        dst[c] += src[c] * scale;
    }
}

#[allow(clippy::too_many_arguments)]
fn cfr_iteration(
    tree: &Tree,
    evals: &[BoardEval],
    node_eval: &[usize],
    regrets: &mut [Vec<f32>],
    strat_sum: &mut [Vec<f32>],
    scratch: &mut [Scratch],
    sigma_buf: &mut [f32],
    n: usize,
    weight: f32,
    update: u8,
) {
    let node = &tree.nodes[n];
    match node.kind {
        Kind::Terminal => {
            let s = &mut scratch[0];
            terminal_values(
                tree, evals, node_eval, n, &s.r0, &s.r1, &mut s.o0, &mut s.o1, &mut s.term,
            );
        }
        Kind::Chance => {
            let k = node.children.len();
            debug_assert!(k >= 6, "chance node needs at least 6 branches");
            let p = 1.0 / (k as f32 - 2.0);
            // Restores the joint (both hands) chance probability: each pair is
            // compatible with k-4 of the k branches.
            let joint = (k as f32 - 2.0) / (k as f32 - 4.0);
            {
                let s = &mut scratch[0];
                s.o0.fill(0.0);
                s.o1.fill(0.0);
            }
            for (i, &ch) in node.children.iter().enumerate() {
                let bit = 1u64 << node.chance_cards[i];
                {
                    let (head, tail) = scratch.split_at_mut(1);
                    let parent = &mut head[0];
                    let child = &mut tail[0];
                    copy_masked(&parent.r0, &mut child.r0, p, bit);
                    copy_masked(&parent.r1, &mut child.r1, p, bit);
                }
                {
                    let (head, tail) = scratch.split_at_mut(1);
                    cfr_iteration(
                        tree, evals, node_eval, regrets, strat_sum, tail, sigma_buf, ch, weight,
                        update,
                    );
                    let parent = &mut head[0];
                    let child = &tail[0];
                    add_scaled(&mut parent.o0, &child.o0, joint);
                    add_scaled(&mut parent.o1, &child.o1, joint);
                }
            }
        }
        Kind::Decision => {
            let na = node.actions.len();
            let player = node.player as usize;
            {
                let s = &mut scratch[0];
                regret_matching(&regrets[n], na, sigma_buf);
                s.sigma[..na * NCOMBOS].copy_from_slice(&sigma_buf[..na * NCOMBOS]);
                s.o0.fill(0.0);
                s.o1.fill(0.0);
                if player as u8 == update {
                    let own = if player == 0 { &s.r0 } else { &s.r1 };
                    let st = &mut strat_sum[n];
                    for c in 0..NCOMBOS {
                        let r = own[c] * weight;
                        if r == 0.0 {
                            continue;
                        }
                        for a in 0..na {
                            st[a * NCOMBOS + c] += r * s.sigma[a * NCOMBOS + c];
                        }
                    }
                }
            }

            for a in 0..na {
                {
                    let (head, tail) = scratch.split_at_mut(1);
                    let parent = &mut head[0];
                    let child = &mut tail[0];
                    if player == 0 {
                        for c in 0..NCOMBOS {
                            child.r0[c] = parent.r0[c] * parent.sigma[a * NCOMBOS + c];
                        }
                        child.r1.copy_from_slice(&parent.r1);
                    } else {
                        child.r0.copy_from_slice(&parent.r0);
                        for c in 0..NCOMBOS {
                            child.r1[c] = parent.r1[c] * parent.sigma[a * NCOMBOS + c];
                        }
                    }
                }
                {
                    let (head, tail) = scratch.split_at_mut(1);
                    cfr_iteration(
                        tree,
                        evals,
                        node_eval,
                        regrets,
                        strat_sum,
                        tail,
                        sigma_buf,
                        node.children[a],
                        weight,
                        update,
                    );
                    let parent = &mut head[0];
                    let child = &tail[0];
                    let (mine, theirs) = if player == 0 {
                        (&child.o0, &child.o1)
                    } else {
                        (&child.o1, &child.o0)
                    };
                    parent.av[a * NCOMBOS..(a + 1) * NCOMBOS].copy_from_slice(mine);
                    let (out_mine, out_theirs) = if player == 0 {
                        (&mut parent.o0, &mut parent.o1)
                    } else {
                        (&mut parent.o1, &mut parent.o0)
                    };
                    for c in 0..NCOMBOS {
                        out_mine[c] += parent.sigma[a * NCOMBOS + c] * mine[c];
                        out_theirs[c] += theirs[c];
                    }
                }
            }

            if player as u8 == update {
                let s = &mut scratch[0];
                let reg = &mut regrets[n];
                let (out, av) = if player == 0 {
                    (&s.o0, &s.av)
                } else {
                    (&s.o1, &s.av)
                };
                for c in 0..NCOMBOS {
                    for a in 0..na {
                        let idx = a * NCOMBOS + c;
                        reg[idx] = (reg[idx] + (av[idx] - out[c])).max(0.0);
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn avg_traverse(
    tree: &Tree,
    evals: &[BoardEval],
    node_eval: &[usize],
    strat: &[Vec<f32>],
    scratch: &mut [Scratch],
    sigma_buf: &mut [f32],
    n: usize,
    br_player: Option<u8>,
) {
    let node = &tree.nodes[n];
    match node.kind {
        Kind::Terminal => {
            let s = &mut scratch[0];
            terminal_values(
                tree, evals, node_eval, n, &s.r0, &s.r1, &mut s.o0, &mut s.o1, &mut s.term,
            );
        }
        Kind::Chance => {
            let k = node.children.len();
            debug_assert!(k >= 6, "chance node needs at least 6 branches");
            let p = 1.0 / (k as f32 - 2.0);
            // Restores the joint (both hands) chance probability: each pair is
            // compatible with k-4 of the k branches.
            let joint = (k as f32 - 2.0) / (k as f32 - 4.0);
            {
                let s = &mut scratch[0];
                s.o0.fill(0.0);
                s.o1.fill(0.0);
            }
            for (i, &ch) in node.children.iter().enumerate() {
                let bit = 1u64 << node.chance_cards[i];
                {
                    let (head, tail) = scratch.split_at_mut(1);
                    let parent = &mut head[0];
                    let child = &mut tail[0];
                    copy_masked(&parent.r0, &mut child.r0, p, bit);
                    copy_masked(&parent.r1, &mut child.r1, p, bit);
                }
                {
                    let (head, tail) = scratch.split_at_mut(1);
                    avg_traverse(
                        tree, evals, node_eval, strat, tail, sigma_buf, ch, br_player,
                    );
                    let parent = &mut head[0];
                    let child = &tail[0];
                    add_scaled(&mut parent.o0, &child.o0, joint);
                    add_scaled(&mut parent.o1, &child.o1, joint);
                }
            }
        }
        Kind::Decision => {
            let na = node.actions.len();
            let player = node.player as usize;
            {
                let s = &mut scratch[0];
                avg_strategy_reg(&strat[n], na, sigma_buf);
                s.sigma[..na * NCOMBOS].copy_from_slice(&sigma_buf[..na * NCOMBOS]);
                s.o0.fill(0.0);
                s.o1.fill(0.0);
            }
            let br_here = br_player == Some(node.player);
            if br_here {
                let mut first = true;
                for a in 0..na {
                    {
                        let (head, tail) = scratch.split_at_mut(1);
                        let parent = &mut head[0];
                        let child = &mut tail[0];
                        child.r0.copy_from_slice(&parent.r0);
                        child.r1.copy_from_slice(&parent.r1);
                    }
                    {
                        let (head, tail) = scratch.split_at_mut(1);
                        avg_traverse(
                            tree,
                            evals,
                            node_eval,
                            strat,
                            tail,
                            sigma_buf,
                            node.children[a],
                            br_player,
                        );
                        let parent = &mut head[0];
                        let child = &tail[0];
                        let (mine, theirs) = if player == 0 {
                            (&child.o0, &child.o1)
                        } else {
                            (&child.o1, &child.o0)
                        };
                        let (out_mine, out_theirs) = if player == 0 {
                            (&mut parent.o0, &mut parent.o1)
                        } else {
                            (&mut parent.o1, &mut parent.o0)
                        };
                        if first {
                            out_mine.copy_from_slice(mine);
                            out_theirs.copy_from_slice(theirs);
                            first = false;
                        } else {
                            for c in 0..NCOMBOS {
                                if mine[c] > out_mine[c] {
                                    out_mine[c] = mine[c];
                                }
                            }
                        }
                    }
                }
            } else {
                for a in 0..na {
                    {
                        let (head, tail) = scratch.split_at_mut(1);
                        let parent = &mut head[0];
                        let child = &mut tail[0];
                        if player == 0 {
                            for c in 0..NCOMBOS {
                                child.r0[c] = parent.r0[c] * parent.sigma[a * NCOMBOS + c];
                            }
                            child.r1.copy_from_slice(&parent.r1);
                        } else {
                            child.r0.copy_from_slice(&parent.r0);
                            for c in 0..NCOMBOS {
                                child.r1[c] = parent.r1[c] * parent.sigma[a * NCOMBOS + c];
                            }
                        }
                    }
                    {
                        let (head, tail) = scratch.split_at_mut(1);
                        avg_traverse(
                            tree,
                            evals,
                            node_eval,
                            strat,
                            tail,
                            sigma_buf,
                            node.children[a],
                            br_player,
                        );
                        let parent = &mut head[0];
                        let child = &tail[0];
                        if player == 0 {
                            for c in 0..NCOMBOS {
                                parent.o0[c] += parent.sigma[a * NCOMBOS + c] * child.o0[c];
                                parent.o1[c] += child.o1[c];
                            }
                        } else {
                            for c in 0..NCOMBOS {
                                parent.o1[c] += parent.sigma[a * NCOMBOS + c] * child.o1[c];
                                parent.o0[c] += child.o0[c];
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_rec(
    solver: &Solver,
    n: usize,
    scratch: &mut [Scratch],
    sigma_buf: &mut [f32],
    parent: &mut [Option<usize>],
    out: &mut [Option<NodeReport>],
) {
    let node = &solver.tree.nodes[n];
    if node.kind == Kind::Decision {
        let na = node.actions.len();
        let player = node.player as usize;
        let mut sigma = vec![0.0f32; na * NCOMBOS];
        solver.avg_strategy(n, &mut sigma);
        let own = if player == 0 { &scratch[0].r0 } else { &scratch[0].r1 };
        let other = if player == 0 { &scratch[0].r1 } else { &scratch[0].r0 };
        let own_mass: f64 = own.iter().map(|x| *x as f64).sum();
        let other_mass: f64 = other.iter().map(|x| *x as f64).sum();
        let mut freq = vec![0.0f64; na];
        for c in 0..NCOMBOS {
            let w = own[c] as f64;
            if w == 0.0 {
                continue;
            }
            for a in 0..na {
                freq[a] += w * sigma[a * NCOMBOS + c] as f64;
            }
        }
        let actions = node
            .actions
            .iter()
            .enumerate()
            .map(|(a, act)| ActionReport {
                label: act.label(node.pot),
                frequency: if own_mass > 0.0 { freq[a] / own_mass } else { 0.0 },
            })
            .collect::<Vec<_>>();
        let mut edges = Vec::new();
        for (a, &child) in node.children.iter().enumerate() {
            for (target, chance) in decision_targets(&solver.tree, child) {
                edges.push(EdgeReport {
                    action_index: a,
                    action: actions[a].label.clone(),
                    frequency: actions[a].frequency,
                    target,
                    chance: chance.map(crate::cards::card_str),
                });
            }
        }
        out[n] = Some(NodeReport {
            id: n,
            parent: parent[n],
            street: node.street,
            board: node.board.iter().map(|c| crate::cards::card_str(*c)).collect(),
            player: node.player,
            pot: node.pot,
            actions,
            edges,
            reach_oop: if player == 0 { own_mass } else { other_mass },
            reach_ip: if player == 1 { own_mass } else { other_mass },
        });

        for (a, &ch) in node.children.iter().enumerate() {
            parent[ch] = Some(n);
            {
                let (head, tail) = scratch.split_at_mut(1);
                let p = &mut head[0];
                let cld = &mut tail[0];
                if player == 0 {
                    for c in 0..NCOMBOS {
                        cld.r0[c] = p.r0[c] * sigma[a * NCOMBOS + c];
                    }
                    cld.r1.copy_from_slice(&p.r1);
                } else {
                    cld.r0.copy_from_slice(&p.r0);
                    for c in 0..NCOMBOS {
                        cld.r1[c] = p.r1[c] * sigma[a * NCOMBOS + c];
                    }
                }
            }
            {
                let (_, tail) = scratch.split_at_mut(1);
                collect_rec(solver, ch, tail, sigma_buf, parent, out);
            }
        }
    } else if node.kind == Kind::Chance {
        let k = node.children.len();
        let p = 1.0 / (k as f32 - 2.0);
        for (i, &ch) in node.children.iter().enumerate() {
            parent[ch] = Some(n);
            let bit = 1u64 << solver.tree.nodes[n].chance_cards[i];
            {
                let (head, tail) = scratch.split_at_mut(1);
                let par = &mut head[0];
                let cld = &mut tail[0];
                copy_masked(&par.r0, &mut cld.r0, p, bit);
                copy_masked(&par.r1, &mut cld.r1, p, bit);
            }
            {
                let (_, tail) = scratch.split_at_mut(1);
                collect_rec(solver, ch, tail, sigma_buf, parent, out);
            }
        }
    }
}

/// Next decision nodes reachable from `n`, crossing at most one chance node.
/// The chance card (if any) is returned so the GUI can label the edge.
fn decision_targets(tree: &Tree, n: usize) -> Vec<(usize, Option<u8>)> {
    fn walk(
        tree: &Tree,
        n: usize,
        cards: &mut Vec<u8>,
        out: &mut Vec<(usize, Option<u8>)>,
    ) {
        match tree.nodes[n].kind {
            Kind::Decision => out.push((n, cards.last().copied())),
            Kind::Chance => {
                let node = &tree.nodes[n];
                for (i, &child) in node.children.iter().enumerate() {
                    cards.push(node.chance_cards[i]);
                    walk(tree, child, cards, out);
                    cards.pop();
                }
            }
            Kind::Terminal => {}
        }
    }
    let mut out = Vec::new();
    walk(tree, n, &mut Vec::new(), &mut out);
    out
}

/// Rake for a finished pot: percentage with an optional cap.
pub fn rake_for(pot: f64, pct: f64, cap: f64) -> f64 {
    if pct <= 0.0 || pot <= 0.0 {
        return 0.0;
    }
    let mut r = pot * pct;
    if cap > 0.0 {
        r = r.min(cap);
    }
    r
}

#[allow(clippy::too_many_arguments)]
fn terminal_values(
    tree: &Tree,
    evals: &[BoardEval],
    node_eval: &[usize],
    n: usize,
    r0: &[f32],
    r1: &[f32],
    o0: &mut [f32],
    o1: &mut [f32],
    ts: &mut TerminalScratch,
) {
    o0.fill(0.0);
    o1.fill(0.0);
    let node = &tree.nodes[n];
    let rake = rake_for(node.pot, tree.config.rake_pct, tree.config.rake_cap);
    match node.terminal {
        Some(Terminal::Showdown) => {
            let be = &evals[node_eval[n]];
            ts.cur0 = [0.0; 52];
            ts.cur1 = [0.0; 52];
            ts.below0 = [0.0; 52];
            ts.below1 = [0.0; 52];
            ts.card0 = [0.0; 52];
            ts.card1 = [0.0; 52];
            ts.total0 = 0.0;
            ts.total1 = 0.0;
            ts.pre0 = 0.0;
            ts.pre1 = 0.0;

            for &ci in &be.order {
                let c = ci as usize;
                let (a, b) = combo_cards(c);
                let (w0, w1) = (r0[c], r1[c]);
                ts.total0 += w0;
                ts.total1 += w1;
                ts.card0[a as usize] += w0;
                ts.card0[b as usize] += w0;
                ts.card1[a as usize] += w1;
                ts.card1[b as usize] += w1;
            }
            let inv0 = node.invested[0] as f32;
            let inv1 = node.invested[1] as f32;
            let rake = rake as f32;
            let mut i = 0usize;
            while i < be.order.len() {
                let g = be.group_of[be.order[i] as usize];
                let mut j = i;
                let mut cw0 = 0.0f32;
                let mut cw1 = 0.0f32;
                while j < be.order.len() && be.group_of[be.order[j] as usize] == g {
                    let c = be.order[j] as usize;
                    let (a, b) = combo_cards(c);
                    let (w0, w1) = (r0[c], r1[c]);
                    ts.cur0[a as usize] += w0;
                    ts.cur0[b as usize] += w0;
                    ts.cur1[a as usize] += w1;
                    ts.cur1[b as usize] += w1;
                    cw0 += w0;
                    cw1 += w1;
                    ts.touched.push(a);
                    ts.touched.push(b);
                    j += 1;
                }
                // Values for this strength group: opponent combos are excluded
                // card-by-card using running per-card weight sums.
                for k in i..j {
                    let c = be.order[k] as usize;
                    let (a, b) = combo_cards(c);
                    let (aa, bb) = (a as usize, b as usize);

                    let beat1 = ts.pre1 - ts.below1[aa] - ts.below1[bb];
                    let above1 = ts.total1 - ts.pre1 - cw1;
                    let lose1 = above1
                        - (ts.card1[aa] - ts.below1[aa] - ts.cur1[aa])
                        - (ts.card1[bb] - ts.below1[bb] - ts.cur1[bb]);
                    let tie1 = cw1 - ts.cur1[aa] - ts.cur1[bb] + r1[c];
                    o0[c] = inv1 * (beat1 - lose1) - rake * (beat1 + 0.5 * tie1.max(0.0));

                    let beat0 = ts.pre0 - ts.below0[aa] - ts.below0[bb];
                    let above0 = ts.total0 - ts.pre0 - cw0;
                    let lose0 = above0
                        - (ts.card0[aa] - ts.below0[aa] - ts.cur0[aa])
                        - (ts.card0[bb] - ts.below0[bb] - ts.cur0[bb]);
                    let tie0 = cw0 - ts.cur0[aa] - ts.cur0[bb] + r0[c];
                    o1[c] = inv0 * (beat0 - lose0) - rake * (beat0 + 0.5 * tie0.max(0.0));
                }
                for &card in &ts.touched {
                    let ci = card as usize;
                    ts.below0[ci] += ts.cur0[ci];
                    ts.below1[ci] += ts.cur1[ci];
                    ts.cur0[ci] = 0.0;
                    ts.cur1[ci] = 0.0;
                }
                ts.touched.clear();
                ts.pre0 += cw0;
                ts.pre1 += cw1;
                i = j;
            }
        }
        Some(Terminal::Fold { winner }) => {
            let mut tot0 = 0.0f64;
            let mut tot1 = 0.0f64;
            let mut card0 = [0.0f64; 52];
            let mut card1 = [0.0f64; 52];
            for c in 0..NCOMBOS {
                let w0 = r0[c] as f64;
                let w1 = r1[c] as f64;
                if w0 == 0.0 && w1 == 0.0 {
                    continue;
                }
                tot0 += w0;
                tot1 += w1;
                let (a, b) = combo_cards(c);
                card0[a as usize] += w0;
                card0[b as usize] += w0;
                card1[a as usize] += w1;
                card1[b as usize] += w1;
            }
            let loser = 1 - winner;
            let stake = node.invested[loser as usize] as f32;
            let win_stake = (stake as f64 - rake) as f32;
            for c in 0..NCOMBOS {
                let (a, b) = combo_cards(c);
                let (aa, bb) = (a as usize, b as usize);
                let excl1 = (tot1 - card1[aa] - card1[bb] + r1[c] as f64) as f32;
                let excl0 = (tot0 - card0[aa] - card0[bb] + r0[c] as f64) as f32;
                if winner == 0 {
                    o0[c] = win_stake * excl1;
                    o1[c] = -stake * excl0;
                } else {
                    o1[c] = win_stake * excl0;
                    o0[c] = -stake * excl1;
                }
            }
        }
        None => {}
    }
}

#[cfg(test)]
mod brute_force_tests {
    use super::*;
    use crate::tree::{Tree, TreeConfig};

    /// Enumerates combo pairs and action paths explicitly with a uniform
    /// strategy, mirroring the chance weighting of the vector solver.
    fn brute(
        tree: &Tree,
        n: usize,
        c0: usize,
        c1: usize,
        streak: f64,
        v0: &mut f64,
        v1: &mut f64,
        w0: f64,
        w1: f64,
    ) {
        let node = &tree.nodes[n];
        match node.kind {
            Kind::Terminal => {
                let bm = board_mask(&node.board);
                if combo_mask(c0) & bm != 0 || combo_mask(c1) & bm != 0 {
                    return;
                }
                let (a0, b0) = combo_cards(c0);
                let (a1, b1) = combo_cards(c1);
                if (1u64 << a0 | 1u64 << b0) & (1u64 << a1 | 1u64 << b1) != 0 {
                    return;
                }
                let inv = node.invested[0];
                let (u0, u1) = match node.terminal.unwrap() {
                    Terminal::Fold { winner } => {
                        if winner == 0 {
                            (node.invested[1], -node.invested[1])
                        } else {
                            (-node.invested[0], node.invested[0])
                        }
                    }
                    Terminal::Showdown => {
                        let s0 = eval::eval7((a0, b0), &node.board);
                        let s1 = eval::eval7((a1, b1), &node.board);
                        if s0 > s1 {
                            (inv, -inv)
                        } else if s0 < s1 {
                            (-inv, inv)
                        } else {
                            (0.0, 0.0)
                        }
                    }
                };
                *v0 += w0 * w1 * streak * u0;
                *v1 += w0 * w1 * streak * u1;
            }
            Kind::Chance => {
                let k = node.children.len() as f64;
                let p = 1.0 / (k - 2.0);
                let joint = (k - 2.0) / (k - 4.0);
                for (i, &ch) in node.children.iter().enumerate() {
                    let bit = 1u64 << node.chance_cards[i];
                    if combo_mask(c0) & bit != 0 || combo_mask(c1) & bit != 0 {
                        continue;
                    }
                    brute(tree, ch, c0, c1, streak * p * joint, v0, v1, w0, w1);
                }
            }
            Kind::Decision => {
                let na = node.actions.len();
                let sigma = 1.0 / na as f64;
                for (a, &ch) in node.children.iter().enumerate() {
                    let _ = a;
                    brute(tree, ch, c0, c1, streak * sigma, v0, v1, w0, w1);
                }
            }
        }
    }

    fn brute_trained(
        solver: &Solver,
        n: usize,
        c0: usize,
        c1: usize,
        streak: f64,
        v0: &mut f64,
        v1: &mut f64,
        w0: f64,
        w1: f64,
        buf: &mut [f32],
    ) {
        let node = &solver.tree.nodes[n];
        match node.kind {
            Kind::Terminal => {
                let bm = board_mask(&node.board);
                if combo_mask(c0) & bm != 0 || combo_mask(c1) & bm != 0 {
                    return;
                }
                let (a0, b0c) = combo_cards(c0);
                let (a1, b1c) = combo_cards(c1);
                if ((1u64 << a0) | (1u64 << b0c)) & ((1u64 << a1) | (1u64 << b1c)) != 0 {
                    return;
                }
                let inv = node.invested[0];
                let (u0, u1) = match node.terminal.unwrap() {
                    Terminal::Fold { winner } => {
                        if winner == 0 {
                            (node.invested[1], -node.invested[1])
                        } else {
                            (-node.invested[0], node.invested[0])
                        }
                    }
                    Terminal::Showdown => {
                        let s0 = eval::eval7((a0, b0c), &node.board);
                        let s1 = eval::eval7((a1, b1c), &node.board);
                        if s0 > s1 {
                            (inv, -inv)
                        } else if s0 < s1 {
                            (-inv, inv)
                        } else {
                            (0.0, 0.0)
                        }
                    }
                };
                *v0 += w0 * w1 * streak * u0;
                *v1 += w0 * w1 * streak * u1;
            }
            Kind::Chance => {
                let k = node.children.len() as f64;
                let p = 1.0 / (k - 2.0);
                let joint = (k - 2.0) / (k - 4.0);
                for (i, &ch) in node.children.iter().enumerate() {
                    let bit = 1u64 << node.chance_cards[i];
                    if combo_mask(c0) & bit != 0 || combo_mask(c1) & bit != 0 {
                        continue;
                    }
                    brute_trained(solver, ch, c0, c1, streak * p * joint, v0, v1, w0, w1, buf);
                }
            }
            Kind::Decision => {
                let na = node.actions.len();
                solver.avg_strategy(n, buf);
                let actor = node.player as usize;
                let combo = if actor == 0 { c0 } else { c1 };
                // Snapshot before recursing: the scratch buffer is shared.
                let probs: Vec<f64> = (0..na)
                    .map(|a| buf[a * NCOMBOS + combo] as f64)
                    .collect();
                for (a, &ch) in node.children.iter().enumerate() {
                    if probs[a] > 0.0 {
                        brute_trained(solver, ch, c0, c1, streak * probs[a], v0, v1, w0, w1, buf);
                    }
                }
            }
            _ => {}
        }
    }

    #[test]
    fn vector_ev_matches_brute_force() {
        let cfg = TreeConfig {
            board: crate::cards::parse_board("AhKd7c2s9h").unwrap(),
            pot: 20.0,
            stack: 80.0,
            bet_sizes: vec![vec![1.0], vec![1.0]],
            max_raises: 1,
            rake_pct: 0.0,
            rake_cap: 0.0,
            runout_samples: 0,
            turn_samples: 0,
            seed: 7,
            max_nodes: 400_000,
        };
        let tree = Tree::build(cfg).unwrap();
        let mut solver = Solver::new(
            tree,
            &Range::from_str("AA").unwrap(),
            &Range::from_str("KK").unwrap(),
        )
        .unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        solver.solve(
            &SolveOptions {
                iterations: 40,
                target_exploitability: 0.0,
                check_every: 20,
                verbose: false,
            },
            &cancel,
            |_| {},
        );
        let mut scratch = solver.scratch_pool();
        let mut sigma = vec![0.0f32; solver.max_actions * NCOMBOS];
        let (ev0, ev1, _, _) = solver.evaluate(&mut scratch, &mut sigma);

        let mut b0 = 0.0;
        let mut b1 = 0.0;
        let mut buf = vec![0.0f32; solver.max_actions * NCOMBOS];
        for c0 in 0..NCOMBOS {
            if solver.root0[c0] == 0.0 {
                continue;
            }
            for c1 in 0..NCOMBOS {
                if solver.root1[c1] == 0.0 {
                    continue;
                }
                let w0 = solver.root0[c0] as f64;
                let w1 = solver.root1[c1] as f64;
                let mut a = 0.0;
                let mut b = 0.0;
                brute_trained(&solver, 0, c0, c1, 1.0, &mut a, &mut b, w0, w1, &mut buf);
                b0 += a;
                b1 += b;
            }
        }
        eprintln!("vector ev0={ev0:.6} ev1={ev1:.6} sum={:.6}", ev0 + ev1);
        eprintln!("brute  b0={b0:.6} b1={b1:.6} sum={:.6}", b0 + b1);
        assert!((ev0 - b0).abs() < 1e-2, "ev0 {ev0} vs {b0}");
        assert!((ev1 - b1).abs() < 1e-2, "ev1 {ev1} vs {b1}");
        assert!((ev0 + ev1).abs() < 1e-3, "vector ev must be zero-sum");
        assert!((b0 + b1).abs() < 1e-9, "brute ev must be zero-sum");
    }
}

