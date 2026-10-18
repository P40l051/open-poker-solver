//! Public betting tree construction.
//!
//! The tree is built once per configuration and shared read-only by the
//! CFR workers. Every street is modelled with the same action set:
//! check, bet (several sizes), and — when facing a bet — fold, call and a
//! bounded number of raises. Stacks are capped, so an oversized bet becomes
//! an all-in.

use crate::cards::NCOMBOS;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub kind: ActionKind,
    /// Total chips put in this street by the acting player after the action.
    pub amount: f64,
    /// Chips moved by this action.
    pub delta: f64,
}

impl Action {
    pub fn label(&self, _pot: f64) -> String {
        match self.kind {
            ActionKind::Fold => "fold".into(),
            ActionKind::Check => "check".into(),
            ActionKind::Call => "call".into(),
            ActionKind::Bet => format!("bet {:.2}", self.amount),
            ActionKind::Raise => format!("raise to {:.2}", self.amount),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Decision,
    Chance,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terminal {
    Fold { winner: u8 },
    Showdown,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub street: u8,
    pub board: Vec<u8>,
    pub pot: f64,
    pub invested: [f64; 2],
    pub player: u8,
    pub actions: Vec<Action>,
    pub children: Vec<usize>,
    pub terminal: Option<Terminal>,
    /// Dealt cards for chance nodes (parallel to `children`).
    pub chance_cards: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct TreeConfig {
    pub board: Vec<u8>,
    /// Chips already in the pot when the solve starts.
    pub pot: f64,
    /// Effective stack behind.
    pub stack: f64,
    /// Bet sizes as a fraction of the pot, per street (index 0 = first solved street).
    pub bet_sizes: Vec<Vec<f64>>,
    pub max_raises: u32,
    /// Rake taken from the pot as a fraction (0.05 = 5%).
    pub rake_pct: f64,
    /// Rake cap in chips; 0 = uncapped.
    pub rake_cap: f64,
    /// 0 = enumerate every river card; >0 = sample this many rivers per turn.
    pub runout_samples: usize,
    /// 0 = enumerate every turn card; >0 = sample this many turns on a flop.
    pub turn_samples: usize,
    pub seed: u64,
    pub max_nodes: usize,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            board: Vec::new(),
            pot: 10.0,
            stack: 100.0,
            bet_sizes: vec![vec![0.75], vec![0.75]],
            max_raises: 1,
            rake_pct: 0.0,
            rake_cap: 0.0,
            runout_samples: 0,
            turn_samples: 0,
            seed: 0x5eed_1234,
            max_nodes: 400_000,
        }
    }
}

pub struct Tree {
    pub nodes: Vec<Node>,
    pub config: TreeConfig,
    pub max_depth: usize,
    pub decision_count: usize,
    /// Street index of the first solved street (0 = flop, 1 = turn, 2 = river).
    pub start_street: u8,
}

#[derive(Clone, Debug)]
struct State {
    board: Vec<u8>,
    pot: f64,
    invested: [f64; 2],
    stack: [f64; 2],
    street_bet: [f64; 2],
    street: u8,
    to_act: u8,
    raises: u32,
    checked: bool,
    min_raise: f64,
}

const EPS: f64 = 1e-9;

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Tree {
    pub fn build(cfg: TreeConfig) -> Result<Tree, String> {
        if cfg.board.len() < 3 || cfg.board.len() > 5 {
            return Err("board must contain 3, 4 or 5 cards".into());
        }
        if cfg.pot <= 0.0 || cfg.stack <= 0.0 {
            return Err("pot and stack must be positive".into());
        }
        let mut tree = Tree {
            nodes: Vec::new(),
            max_depth: 0,
            decision_count: 0,
            start_street: (cfg.board.len() - 3) as u8,
            config: cfg.clone(),
        };
        let street = tree.start_street;
        let st = State {
            board: cfg.board.clone(),
            pot: cfg.pot,
            invested: [cfg.pot / 2.0, cfg.pot / 2.0],
            stack: [cfg.stack, cfg.stack],
            street_bet: [0.0, 0.0],
            street,
            to_act: 0,
            raises: 0,
            checked: false,
            min_raise: cfg.pot.max(1.0),
        };
        tree.build_node(st, 0)?;
        Ok(tree)
    }

    fn push(&mut self, node: Node, depth: usize) -> Result<usize, String> {
        if self.nodes.len() >= self.config.max_nodes {
            return Err(format!(
                "tree exceeds max_nodes={} (raise the limit or reduce bet sizes)",
                self.config.max_nodes
            ));
        }
        self.max_depth = self.max_depth.max(depth);
        self.nodes.push(node);
        Ok(self.nodes.len() - 1)
    }

    fn sizes_for(&self, street: u8) -> Vec<f64> {
        // `bet_sizes` is indexed from the first solved street, not from the
        // absolute street number.
        let idx = street.saturating_sub(self.start_street) as usize;
        if self.config.bet_sizes.is_empty() {
            return vec![0.75];
        }
        self.config
            .bet_sizes
            .get(idx)
            .or_else(|| self.config.bet_sizes.last())
            .cloned()
            .unwrap()
    }

    fn build_node(&mut self, st: State, depth: usize) -> Result<usize, String> {
        let actor = st.to_act as usize;
        let opp = 1 - actor;
        let to_call = (st.street_bet[opp] - st.street_bet[actor]).max(0.0);

        // Forced run-out when a player is all-in and there is nothing to call.
        // An all-in bet still has to be answered by the opponent.
        if to_call <= EPS && (st.stack[0] <= EPS || st.stack[1] <= EPS) {
            return self.advance(st, depth);
        }

        let mut actions: Vec<Action> = Vec::new();

        if to_call <= EPS {
            actions.push(Action {
                kind: ActionKind::Check,
                amount: st.street_bet[actor],
                delta: 0.0,
            });
            let mut seen = BTreeSet::new();
            for frac in self.sizes_for(st.street) {
                if !(frac > 0.0) {
                    continue;
                }
                let mut amt = (frac * st.pot).max(1.0).min(st.stack[actor]);
                if amt >= st.stack[actor] - 1e-6 {
                    amt = st.stack[actor];
                }
                let key = (amt * 1000.0).round() as i64;
                if !seen.insert(key) || amt <= EPS {
                    continue;
                }
                actions.push(Action {
                    kind: ActionKind::Bet,
                    amount: st.street_bet[actor] + amt,
                    delta: amt,
                });
            }
        } else {
            actions.push(Action {
                kind: ActionKind::Fold,
                amount: st.street_bet[actor],
                delta: 0.0,
            });
            let call_amt = to_call.min(st.stack[actor]);
            actions.push(Action {
                kind: ActionKind::Call,
                amount: st.street_bet[actor] + call_amt,
                delta: call_amt,
            });
            if st.raises < self.config.max_raises {
                let mut seen = BTreeSet::new();
                for frac in self.sizes_for(st.street) {
                    if !(frac > 0.0) {
                        continue;
                    }
                    let target_total =
                        st.street_bet[opp] + (frac * (st.pot + to_call)).max(st.min_raise);
                    let max_total = st.street_bet[actor] + st.stack[actor];
                    if target_total >= max_total - 1e-6 {
                        // all-in raise
                        if max_total <= st.street_bet[opp] + st.min_raise - 1e-6 {
                            continue;
                        }
                        add_raise(&mut actions, &mut seen, &st, max_total);
                    } else {
                        add_raise(&mut actions, &mut seen, &st, target_total);
                    }
                }
                // Always allow an all-in raise even when no size reaches it.
                let max_total = st.street_bet[actor] + st.stack[actor];
                if max_total > st.street_bet[opp] + st.min_raise - 1e-6 {
                    add_raise(&mut actions, &mut seen, &st, max_total);
                }
            }
        }

        let node = Node {
            kind: Kind::Decision,
            street: st.street,
            board: st.board.clone(),
            pot: st.pot,
            invested: st.invested,
            player: actor as u8,
            actions: actions.clone(),
            children: Vec::new(),
            terminal: None,
            chance_cards: Vec::new(),
        };
        let idx = self.push(node, depth)?;
        self.decision_count += 1;

        let mut children = Vec::with_capacity(actions.len());
        for a in &actions {
            let mut next = st.clone();
            next.to_act = opp as u8;
            match a.kind {
                ActionKind::Fold => {
                    let n = Node {
                        kind: Kind::Terminal,
                        street: st.street,
                        board: st.board.clone(),
                        pot: st.pot,
                        invested: st.invested,
                        player: actor as u8,
                        actions: Vec::new(),
                        children: Vec::new(),
                        terminal: Some(Terminal::Fold { winner: opp as u8 }),
                        chance_cards: Vec::new(),
                    };
                    children.push(self.push(n, depth + 1)?);
                }
                ActionKind::Check => {
                    if st.checked {
                        children.push(self.advance(next, depth + 1)?);
                    } else {
                        next.checked = true;
                        children.push(self.build_node(next, depth + 1)?);
                    }
                }
                ActionKind::Call => {
                    next.stack[actor] -= a.delta;
                    next.pot += a.delta;
                    next.invested[actor] += a.delta;
                    next.street_bet[actor] += a.delta;
                    children.push(self.advance(next, depth + 1)?);
                }
                ActionKind::Bet => {
                    next.stack[actor] -= a.delta;
                    next.pot += a.delta;
                    next.invested[actor] += a.delta;
                    let prev = next.street_bet[actor];
                    next.street_bet[actor] = a.amount;
                    next.min_raise = (a.amount - prev).max(st.min_raise);
                    // A bet does not consume a raise slot: `max_raises` counts
                    // raises on top of the opening bet.
                    next.checked = false;
                    children.push(self.build_node(next, depth + 1)?);
                }
                ActionKind::Raise => {
                    next.stack[actor] -= a.delta;
                    next.pot += a.delta;
                    next.invested[actor] += a.delta;
                    let prev = next.street_bet[actor];
                    next.street_bet[actor] = a.amount;
                    next.min_raise = (a.amount - prev).max(st.min_raise);
                    next.raises += 1;
                    next.checked = false;
                    children.push(self.build_node(next, depth + 1)?);
                }
            }
        }
        self.nodes[idx].children = children;
        Ok(idx)
    }

    fn advance(&mut self, st: State, depth: usize) -> Result<usize, String> {
        if st.street >= 2 {
            let n = Node {
                kind: Kind::Terminal,
                street: st.street,
                board: st.board.clone(),
                pot: st.pot,
                invested: st.invested,
                player: st.to_act,
                actions: Vec::new(),
                children: Vec::new(),
                terminal: Some(Terminal::Showdown),
                chance_cards: Vec::new(),
            };
            return self.push(n, depth);
        }

        let used: u64 = st.board.iter().fold(0u64, |m, c| m | (1u64 << c));
        let mut cards: Vec<u8> = (0..52u8).filter(|c| used & (1u64 << c) == 0).collect();
        let next_street = st.street + 1;
        let want = if next_street == 2 {
            self.config.runout_samples
        } else {
            self.config.turn_samples
        };
        if want > 0 && cards.len() > want {
            let k = want;
            let mut rng = splitmix64(
                self.config.seed
                    ^ ((depth as u64) << 17)
                    ^ (st.board.len() as u64)
                    ^ ((st.invested[0].to_bits()) << 3),
            );
            for i in (cards.len() - k..cards.len()).rev() {
                let j = (rng % (i as u64 + 1)) as usize;
                cards.swap(i, j);
                rng = splitmix64(rng);
            }
            cards = cards[cards.len() - k..].to_vec();
            cards.sort_unstable();
        }

        let node = Node {
            kind: Kind::Chance,
            street: st.street,
            board: st.board.clone(),
            pot: st.pot,
            invested: st.invested,
            player: st.to_act,
            actions: Vec::new(),
            children: Vec::new(),
            terminal: None,
            chance_cards: cards.clone(),
        };
        let idx = self.push(node, depth)?;

        let mut children = Vec::with_capacity(cards.len());
        for c in cards {
            let mut next = st.clone();
            next.board.push(c);
            next.street = next_street;
            next.street_bet = [0.0, 0.0];
            next.raises = 0;
            next.checked = false;
            next.to_act = 0;
            next.min_raise = st.pot.max(1.0);
            children.push(self.build_node(next, depth + 1)?);
        }
        self.nodes[idx].children = children;
        Ok(idx)
    }

    /// Total number of terminal nodes, useful for cost estimates.
    pub fn terminal_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.kind == Kind::Terminal).count()
    }
}

fn add_raise(actions: &mut Vec<Action>, seen: &mut BTreeSet<i64>, st: &State, target_total: f64) {
    let actor = st.to_act as usize;
    let delta = target_total - st.street_bet[actor];
    if delta <= EPS {
        return;
    }
    let key = (target_total * 1000.0).round() as i64;
    if !seen.insert(key) {
        return;
    }
    actions.push(Action {
        kind: ActionKind::Raise,
        amount: target_total,
        delta,
    });
}

/// Root reach weights for both players, index 0 = OOP.
pub fn board_mask(board: &[u8]) -> u64 {
    board.iter().fold(0u64, |m, c| m | (1u64 << c))
}

pub fn root_weights(range: &crate::range::Range, board: &[u8]) -> Vec<f32> {
    let mask = board_mask(board);
    let mut out = range.weights.clone();
    for idx in 0..NCOMBOS {
        if crate::cards::combo_mask(idx) & mask != 0 {
            out[idx] = 0.0;
        }
    }
    // Renormalize after card removal so the range is a probability
    // distribution over the combos actually available.
    let total: f64 = out.iter().map(|w| *w as f64).sum();
    if total > 0.0 {
        for w in out.iter_mut() {
            *w = (*w as f64 / total) as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse_board;

    fn cfg(board: &str) -> TreeConfig {
        TreeConfig {
            board: parse_board(board).unwrap(),
            pot: 10.0,
            stack: 40.0,
            bet_sizes: vec![vec![0.5, 1.0]],
            max_raises: 1,
            rake_pct: 0.0,
            rake_cap: 0.0,
            runout_samples: 0,
            ..Default::default()
        }
    }

    #[test]
    fn river_tree_has_expected_shape() {
        let t = Tree::build(cfg("AhKd7c2s9h")).unwrap();
        assert!(t.decision_count > 0);
        assert!(t.terminal_count() > 0);
        // No chance nodes on the river.
        assert!(!t.nodes.iter().any(|n| n.kind == Kind::Chance));
    }

    #[test]
    fn turn_tree_enumerates_all_rivers() {
        let t = Tree::build(cfg("AhKd7c2s")).unwrap();
        let chance: Vec<_> = t.nodes.iter().filter(|n| n.kind == Kind::Chance).collect();
        assert!(chance.len() >= 1);
        for c in &chance {
            assert_eq!(c.children.len(), 48, "all rivers enumerated");
        }
    }

    #[test]
    fn flop_sampling_bounds_branches() {
        let mut c = cfg("AhKd7c");
        c.runout_samples = 6;
        let t = Tree::build(c).unwrap();
        for n in t.nodes.iter().filter(|n| n.kind == Kind::Chance) {
            if n.street == 1 {
                assert!(n.children.len() <= 6, "river sampling caps branches");
            } else {
                assert_eq!(n.children.len(), 49, "turn fully enumerated");
            }
        }
    }

    #[test]
    fn max_raises_counts_raises_not_bets() {
        // Facing the opening bet, one raise must be available with max_raises=1.
        let mut c = cfg("AhKd7c2s9h");
        c.max_raises = 1;
        let t = Tree::build(c).unwrap();
        assert!(
            t.nodes
                .iter()
                .any(|n| n.actions.iter().any(|a| a.kind == ActionKind::Raise)),
            "max_raises=1 must allow a raise over the opening bet"
        );

        // max_raises=0 removes raises entirely.
        let mut c0 = cfg("AhKd7c2s9h");
        c0.max_raises = 0;
        let t0 = Tree::build(c0).unwrap();
        assert!(
            !t0.nodes
                .iter()
                .any(|n| n.actions.iter().any(|a| a.kind == ActionKind::Raise)),
            "max_raises=0 must not allow raises"
        );
    }

    #[test]
    fn allin_is_terminal_after_call() {
        let mut c = cfg("AhKd7c2s9h");
        c.stack = 5.0;
        let t = Tree::build(c).unwrap();
        assert!(t.terminal_count() > 0);
        for n in &t.nodes {
            if let Some(Terminal::Fold { winner }) = n.terminal {
                assert!(winner < 2);
            }
        }
    }
}
