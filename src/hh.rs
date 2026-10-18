//! PokerStars hand-history import.
//!
//! Parses a pasted hand and derives a heads-up postflop spot: board at the
//! analysed street, pot and effective stack in big blinds, the two players
//! involved and the bet sizes actually used. Ranges cannot be inferred from a
//! single hand, so they stay user supplied.

use crate::cards::{card_str, parse_card};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
    PostSb,
    PostBb,
    PostAnte,
    Return,
    Collect,
    Show,
    Other,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub player: String,
    pub kind: ActionKind,
    /// Chips added by the action, or the "to" amount for a raise.
    pub amount: f64,
    pub to: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Street {
    pub name: String,
    pub cards: Vec<u8>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct Seat {
    pub number: u32,
    pub name: String,
    pub stack: f64,
}

#[derive(Clone, Debug)]
pub struct Hand {
    pub sb: f64,
    pub bb: f64,
    pub seats: Vec<Seat>,
    pub button: u32,
    pub hero: Option<String>,
    pub hero_cards: Vec<u8>,
    pub streets: Vec<Street>,
    pub total_pot: f64,
    pub rake: f64,
}

#[derive(Clone, Debug)]
pub struct Spot {
    pub street: String,
    pub board: Vec<u8>,
    pub pot_bb: f64,
    pub stack_bb: f64,
    pub pot_chips: f64,
    pub stack_chips: f64,
    pub oop: String,
    pub ip: String,
    pub oop_position: String,
    pub ip_position: String,
    pub preflop: PreflopLine,
    pub bet_sizes: Vec<f64>,
    pub hero: Option<String>,
    pub hero_cards: Vec<u8>,
    pub sb: f64,
    pub bb: f64,
    pub rake: f64,
    pub total_pot: f64,
    pub rake_pct_observed: f64,
    /// Informational messages (rake, rounding) that do not affect validity.
    pub notes: Vec<String>,
    /// Real limitations of the derived spot.
    pub warnings: Vec<String>,
}

/// The kind of pot created by the preflop action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PotType {
    #[default]
    Limped,
    SingleRaised,
    ThreeBet,
    FourBetPlus,
}

impl PotType {
    pub fn short(self) -> &'static str {
        match self {
            PotType::Limped => "Limped",
            PotType::SingleRaised => "SRP",
            PotType::ThreeBet => "3BP",
            PotType::FourBetPlus => "4BP+",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PotType::Limped => "limped pot",
            PotType::SingleRaised => "single-raised pot",
            PotType::ThreeBet => "3-bet pot",
            PotType::FourBetPlus => "4-bet+ pot",
        }
    }
}

/// A player as seen at preflop, with the position derived from the button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreflopPlayer {
    pub name: String,
    pub position: String,
}

impl PreflopPlayer {
    pub fn label(&self) -> String {
        if self.position.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.position, self.name)
        }
    }
}

/// Everything the import can know about the preflop action.
///
/// `opener` is the original raiser (first raise). `last_aggressor` is the last
/// player to raise preflop: in a 3-bet pot it is the 3-bettor, not the opener.
#[derive(Clone, Debug, Default)]
pub struct PreflopLine {
    pub opener: Option<PreflopPlayer>,
    pub last_aggressor: Option<PreflopPlayer>,
    pub open_to: Option<f64>,
    pub open_to_bb: Option<f64>,
    pub callers: Vec<PreflopPlayer>,
    pub pot_type: PotType,
    pub raises: usize,
}

impl PreflopLine {
    pub fn summary(&self) -> String {
        match self.pot_type {
            PotType::Limped => {
                if self.callers.is_empty() {
                    "limped pot, no preflop raise".into()
                } else {
                    format!(
                        "limped pot, {} caller{}",
                        self.callers.len(),
                        if self.callers.len() == 1 { "" } else { "s" }
                    )
                }
            }
            PotType::SingleRaised => {
                let opener = self
                    .opener
                    .as_ref()
                    .map(|p| p.position.as_str())
                    .unwrap_or("?");
                let size = self
                    .open_to_bb
                    .map(|x| format!("{x:.2} bb"))
                    .unwrap_or_else(|| "?".into());
                let callers = if self.callers.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", {} call{}",
                        self.callers
                            .iter()
                            .map(|p| p.position.as_str())
                            .collect::<Vec<_>>()
                            .join("/"),
                        if self.callers.len() == 1 { "" } else { "s" }
                    )
                };
                format!("{opener} opens {size}{callers} -> SRP")
            }
            PotType::ThreeBet => {
                let opener = self
                    .opener
                    .as_ref()
                    .map(|p| p.position.as_str())
                    .unwrap_or("?");
                let agg = self
                    .last_aggressor
                    .as_ref()
                    .map(|p| p.position.as_str())
                    .unwrap_or("?");
                let size = self
                    .open_to_bb
                    .map(|x| format!("{x:.2} bb"))
                    .unwrap_or_else(|| "?".into());
                format!("{opener} opens {size}, {agg} 3-bets -> 3BP")
            }
            PotType::FourBetPlus => {
                let opener = self
                    .opener
                    .as_ref()
                    .map(|p| p.position.as_str())
                    .unwrap_or("?");
                let agg = self
                    .last_aggressor
                    .as_ref()
                    .map(|p| p.position.as_str())
                    .unwrap_or("?");
                format!("{opener} opens, {agg} 4-bets+ -> 4BP+")
            }
        }
    }

    /// Distinguishes the original raiser from the last preflop aggressor.
    pub fn roles(&self) -> String {
        match (&self.opener, &self.last_aggressor) {
            (Some(o), Some(a)) if o.name != a.name => {
                format!("opener {} / preflop aggressor {}", o.label(), a.label())
            }
            (Some(o), _) => format!("preflop raiser {}", o.label()),
            (None, Some(a)) => format!("preflop aggressor {}", a.label()),
            (None, None) => "no preflop raise".into(),
        }
    }
}

/// Extracts a number from a fragment such as `€0.02` or `$1,25`.
pub fn amount(fragment: &str) -> Option<f64> {
    let cleaned: String = fragment
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    let normalized = if cleaned.contains('.') {
        cleaned.replace(',', "")
    } else {
        cleaned.replace(',', ".")
    };
    normalized.parse().ok()
}

fn parse_blinds(line: &str) -> Option<(f64, f64)> {
    // "(€0.01/€0.02 EUR)" or "($0.50/$1 USD)"
    let open = line.find('(')?;
    let close = line[open..].find(')')? + open;
    let inner = &line[open + 1..close];
    let (a, b) = inner.split_once('/')?;
    let x = amount(a)?;
    let y = amount(b)?;
    if x > 0.0 && y > 0.0 {
        Some((x, y))
    } else {
        None
    }
}

fn parse_board_groups(line: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        let Some(end_rel) = rest[start..].find(']') else {
            break;
        };
        let end = start + end_rel;
        let inner = &rest[start + 1..end];
        for tok in inner.split_whitespace() {
            if let Some(c) = parse_card(tok) {
                out.push(c);
            }
        }
        rest = &rest[end + 1..];
    }
    out
}

pub fn parse(text: &str) -> Result<Hand, String> {
    let mut sb = 0.0;
    let mut bb = 0.0;
    let mut seats: Vec<Seat> = Vec::new();
    let mut button = 0u32;
    let mut hero = None;
    let mut hero_cards = Vec::new();
    // Preflop exists from the first blind post; the HOLE CARDS marker only
    // documents the transition.
    let mut streets: Vec<Street> = vec![Street {
        name: "preflop".into(),
        cards: Vec::new(),
        actions: Vec::new(),
    }];
    let mut total_pot = 0.0;
    let mut rake = 0.0;
    let mut seen_hand = false;
    let mut in_summary = false;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if in_summary {
            if line.starts_with("Total pot") {
                if let Some(part) = line.split('|').next() {
                    total_pot = amount(part).unwrap_or(0.0);
                }
                if let Some((_, r)) = line.split_once('|') {
                    rake = amount(r).unwrap_or(0.0);
                }
            }
            continue;
        }
        if line.starts_with("PokerStars Hand #") || line.starts_with("PokerStars Zoom Hand #") {
            seen_hand = true;
            if let Some((a, b)) = parse_blinds(line) {
                sb = a;
                bb = b;
            }
            continue;
        }
        if line.starts_with("*** SUMMARY ***") {
            in_summary = true;
            continue;
        }
        if line.starts_with("PokerStars") {
            continue;
        }
        if let Some(pos) = line.find("Seat #") {
            let digits: String = line[pos + 6..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse() {
                button = n;
            }
            continue;
        }
        if line.starts_with("Table ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Seat ") {
            if let Some((num, tail)) = rest.split_once(':') {
                if let Ok(number) = num.trim().parse::<u32>() {
                    let tail = tail.trim();
                    if let Some(open) = tail.rfind('(') {
                        let name = tail[..open].trim().to_string();
                        let stack = amount(&tail[open..]).unwrap_or(0.0);
                        seats.push(Seat {
                            number,
                            name,
                            stack,
                        });
                    }
                }
            }
            continue;
        }
        if line.starts_with("Dealt to ") {
            if let (Some(open), Some(close)) = (line.find('['), line.find(']')) {
                let name = line["Dealt to ".len()..open].trim().to_string();
                hero = Some(name);
                hero_cards = line[open + 1..close]
                    .split_whitespace()
                    .filter_map(parse_card)
                    .collect();
            }
            continue;
        }
        if line.starts_with("*** HOLE CARDS ***") {
            continue;
        }
        if line.starts_with("*** FLOP ***") {
            streets.push(Street {
                name: "flop".into(),
                cards: parse_board_groups(line),
                actions: Vec::new(),
            });
            continue;
        }
        if line.starts_with("*** TURN ***") {
            streets.push(Street {
                name: "turn".into(),
                cards: parse_board_groups(line),
                actions: Vec::new(),
            });
            continue;
        }
        if line.starts_with("*** RIVER ***") {
            streets.push(Street {
                name: "river".into(),
                cards: parse_board_groups(line),
                actions: Vec::new(),
            });
            continue;
        }
        if line.starts_with("***") {
            continue;
        }
        if line.starts_with("Total pot") {
            if let Some(part) = line.split('|').next() {
                total_pot = amount(part).unwrap_or(0.0);
            }
            if let Some((_, r)) = line.split_once('|') {
                rake = amount(r).unwrap_or(0.0);
            }
            continue;
        }
        if line.starts_with("Board [") {
            continue;
        }

        // Uncalled bet returned.
        if line.starts_with("Uncalled bet") {
            if let Some(name) = line.split("returned to ").nth(1) {
                let name = name.trim().to_string();
                let amt = amount(line).unwrap_or(0.0);
                if let Some(st) = streets.last_mut() {
                    st.actions.push(Action {
                        player: name,
                        kind: ActionKind::Return,
                        amount: amt,
                        to: None,
                    });
                }
            }
            continue;
        }
        if let Some(name) = line.split(" collected ").next() {
            if line.contains(" collected ") {
                let amt = amount(line).unwrap_or(0.0);
                if let Some(st) = streets.last_mut() {
                    st.actions.push(Action {
                        player: name.trim().to_string(),
                        kind: ActionKind::Collect,
                        amount: amt,
                        to: None,
                    });
                }
                continue;
            }
        }

        let Some((name, rest)) = line.split_once(": ") else {
            continue;
        };
        let name = name.trim().to_string();
        let rest = rest.trim();
        let lower = rest.to_ascii_lowercase();
        let (kind, amt, to) = if lower.starts_with("folds") {
            (ActionKind::Fold, 0.0, None)
        } else if lower.starts_with("checks") {
            (ActionKind::Check, 0.0, None)
        } else if lower.starts_with("calls ") {
            (ActionKind::Call, amount(rest).unwrap_or(0.0), None)
        } else if lower.starts_with("bets ") {
            (ActionKind::Bet, amount(rest).unwrap_or(0.0), None)
        } else if lower.starts_with("raises ") {
            let to = rest
                .split(" to ")
                .nth(1)
                .and_then(amount)
                .or_else(|| amount(rest));
            (ActionKind::Raise, 0.0, to)
        } else if lower.starts_with("posts small blind") {
            (ActionKind::PostSb, amount(rest).unwrap_or(0.0), None)
        } else if lower.starts_with("posts big blind") {
            (ActionKind::PostBb, amount(rest).unwrap_or(0.0), None)
        } else if lower.starts_with("posts the ante") {
            (ActionKind::PostAnte, amount(rest).unwrap_or(0.0), None)
        } else if lower.starts_with("shows") || lower.starts_with("mucked") {
            (ActionKind::Show, 0.0, None)
        } else if lower.starts_with("doesn't show") || lower.starts_with("sits out") || lower.starts_with("is sitting out") {
            (ActionKind::Other, 0.0, None)
        } else {
            (ActionKind::Other, 0.0, None)
        };

        let Some(st) = streets.last_mut() else {
            continue;
        };
        st.actions.push(Action {
            player: name,
            kind,
            amount: amt,
            to,
        });
    }

    if !seen_hand {
        return Err("this does not look like a PokerStars hand history".into());
    }
    if streets.is_empty() {
        return Err("no streets found: missing '*** HOLE CARDS ***'".into());
    }
    if bb <= 0.0 {
        return Err("could not read the blinds from the header".into());
    }
    Ok(Hand {
        sb,
        bb,
        seats,
        button,
        hero,
        hero_cards,
        streets,
        total_pot,
        rake,
    })
}

impl Hand {
    /// Circular order of occupied seats starting at the button.
    ///
    /// 0 = button, 1 = small blind, 2 = big blind, ... This handles tables with
    /// empty seats correctly: only occupied seat numbers are wrapped.
    fn seat_order(&self, seat: u32) -> Option<usize> {
        let mut numbers: Vec<u32> = self.seats.iter().map(|s| s.number).collect();
        numbers.sort_unstable();
        numbers.dedup();
        let n = numbers.len();
        if n == 0 {
            return None;
        }
        let pos = numbers.iter().position(|x| *x == seat)?;
        let btn = numbers.iter().position(|x| *x == self.button)?;
        Some((pos + n - btn) % n)
    }

    fn order_key(&self, seat: u32) -> i64 {
        // Postflop action order: the seat after the button acts first, the
        // button acts last. `seat_order` is 0 at the button.
        let n = self.seats.len().max(2) as i64;
        self.seat_order(seat)
            .map(|x| ((x as i64 + n - 1) % n) as i64)
            .unwrap_or(i64::MAX)
    }

    /// Standard position label for a seat. Labels are approximate for unusual
    /// table sizes; 2-9 handed use the usual names.
    fn position_label(&self, seat: u32) -> String {
        let order = match self.seat_order(seat) {
            Some(o) => o,
            None => return format!("Seat {seat}"),
        };
        let names: &[&str] = match self.seats.len() {
            2 => &["BTN", "BB"],
            3 => &["BTN", "SB", "BB"],
            4 => &["BTN", "SB", "BB", "CO"],
            5 => &["BTN", "SB", "BB", "UTG", "CO"],
            6 => &["BTN", "SB", "BB", "UTG", "HJ", "CO"],
            7 => &["BTN", "SB", "BB", "UTG", "MP", "HJ", "CO"],
            8 => &["BTN", "SB", "BB", "UTG", "UTG+1", "MP", "HJ", "CO"],
            9 => &["BTN", "SB", "BB", "UTG", "UTG+1", "MP", "MP+1", "HJ", "CO"],
            10 => &["BTN", "SB", "BB", "UTG", "UTG+1", "UTG+2", "MP", "MP+1", "HJ", "CO"],
            _ => &[],
        };
        names
            .get(order)
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| format!("Seat {seat}"))
    }

    fn preflop_player(&self, name: &str) -> PreflopPlayer {
        let position = self
            .seats
            .iter()
            .find(|s| s.name == name)
            .map(|s| self.position_label(s.number))
            .unwrap_or_default();
        PreflopPlayer {
            name: name.to_string(),
            position,
        }
    }

    /// Classifies the preflop action: original raiser, last aggressor, open
    /// size, callers and pot type.
    pub fn preflop_line(&self) -> PreflopLine {
        let st = match self.streets.first() {
            Some(s) => s,
            None => return PreflopLine::default(),
        };

        let raises: Vec<&Action> = st
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Raise)
            .collect();
        let opener = raises.first().map(|a| self.preflop_player(&a.player));
        let last_aggressor = raises.last().map(|a| self.preflop_player(&a.player));
        let open_to = raises
            .first()
            .and_then(|a| a.to.or(Some(a.amount)))
            .filter(|x| *x > 0.0);
        let open_to_bb = open_to.map(|x| x / self.bb);

        let folded_preflop: Vec<&str> = st
            .actions
            .iter()
            .filter(|a| a.kind == ActionKind::Fold)
            .map(|a| a.player.as_str())
            .collect();
        let mut callers: Vec<PreflopPlayer> = Vec::new();
        for a in &st.actions {
            if a.kind == ActionKind::Call
                && !folded_preflop.contains(&a.player.as_str())
                && !callers.iter().any(|c| c.name == a.player)
            {
                callers.push(self.preflop_player(&a.player));
            }
        }

        callers.sort_by_key(|c| {
            self.seats
                .iter()
                .find(|s| s.name == c.name)
                .map(|s| self.order_key(s.number))
                .unwrap_or(i64::MAX)
        });

        let pot_type = match raises.len() {
            0 => PotType::Limped,
            1 => PotType::SingleRaised,
            2 => PotType::ThreeBet,
            _ => PotType::FourBetPlus,
        };

        PreflopLine {
            opener,
            last_aggressor,
            open_to,
            open_to_bb,
            callers,
            pot_type,
            raises: raises.len(),
        }
    }

    /// Builds a heads-up postflop spot at the last street with voluntary action.
    pub fn spot(&self) -> Result<Spot, String> {
        if self.streets.len() < 2 {
            return Err("the hand ended preflop: no postflop spot to solve".into());
        }

        let mut contrib: HashMap<String, f64> = self
            .seats
            .iter()
            .map(|s| (s.name.clone(), 0.0))
            .collect();
        let mut folded: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        let preflop = self.preflop_line();
        let bb = self.bb;

        // Pot before each street, plus per-street action sizes.
        let mut pot_before: Vec<f64> = Vec::new();
        let mut street_sizes: Vec<Vec<f64>> = Vec::new();

        for (idx, st) in self.streets.iter().enumerate() {
            let pot = contrib.values().sum::<f64>();
            pot_before.push(pot);
            let mut sizes = Vec::new();
            let mut street_contrib: HashMap<String, f64> = HashMap::new();
            let mut running_pot = pot;

            for a in &st.actions {
                match a.kind {
                    ActionKind::PostSb | ActionKind::PostBb | ActionKind::PostAnte | ActionKind::Call => {
                        *contrib.entry(a.player.clone()).or_insert(0.0) += a.amount;
                        *street_contrib.entry(a.player.clone()).or_insert(0.0) += a.amount;
                        running_pot += a.amount;
                    }
                    ActionKind::Bet => {
                        *contrib.entry(a.player.clone()).or_insert(0.0) += a.amount;
                        *street_contrib.entry(a.player.clone()).or_insert(0.0) += a.amount;
                        if running_pot > 0.0 {
                            sizes.push(round2(a.amount / running_pot));
                        }
                        running_pot += a.amount;
                    }
                    ActionKind::Raise => {
                        let to = a.to.unwrap_or(a.amount);
                        let already = *street_contrib.get(&a.player).unwrap_or(&0.0);
                        let delta = (to - already).max(0.0);
                        *contrib.entry(a.player.clone()).or_insert(0.0) += delta;
                        street_contrib.insert(a.player.clone(), to);
                        if running_pot > 0.0 {
                            sizes.push(round2(delta / running_pot));
                        }
                        running_pot += delta;
                    }
                    ActionKind::Fold => folded.push(a.player.clone()),
                    ActionKind::Return => {
                        *contrib.entry(a.player.clone()).or_insert(0.0) =
                            (contrib.get(&a.player).copied().unwrap_or(0.0) - a.amount).max(0.0);
                    }
                    _ => {}
                }
            }
            street_sizes.push(sizes);
            let _ = idx;
        }

        // Choose the last street that has at least two players and some action.
        let mut chosen: Option<usize> = None;
        let mut folded_before: Vec<String> = Vec::new();
        for (idx, st) in self.streets.iter().enumerate() {
            let active: Vec<&Seat> = self
                .seats
                .iter()
                .filter(|s| !folded_before.contains(&s.name) && !s.name.is_empty())
                .collect();
            let voluntary = st.actions.iter().any(|a| {
                matches!(
                    a.kind,
                    ActionKind::Check | ActionKind::Call | ActionKind::Bet | ActionKind::Raise
                )
            });
            if idx > 0 && active.len() >= 2 && voluntary {
                chosen = Some(idx);
            }
            for a in &st.actions {
                if a.kind == ActionKind::Fold {
                    folded_before.push(a.player.clone());
                }
            }
        }
        let idx = chosen.ok_or_else(|| "no postflop betting spot found in this hand".to_string())?;
        let street = &self.streets[idx];
        let pot_chips = pot_before[idx];
        if pot_chips <= 0.0 {
            return Err("could not reconstruct the pot at the analysed street".into());
        }

        // Active players at the start of the analysed street.
        let mut active: Vec<&Seat> = Vec::new();
        {
            let mut out: Vec<String> = Vec::new();
            for st in &self.streets[..idx] {
                for a in &st.actions {
                    if a.kind == ActionKind::Fold {
                        out.push(a.player.clone());
                    }
                }
            }
            for s in &self.seats {
                if !out.contains(&s.name) {
                    active.push(s);
                }
            }
        }
        active.sort_by_key(|s| self.order_key(s.number));

        // Pick the two players: aggressor + next active in order, else first two.
        let aggressor = street
            .actions
            .iter()
            .rev()
            .find(|a| matches!(a.kind, ActionKind::Bet | ActionKind::Raise))
            .map(|a| a.player.clone());
        let names: Vec<String> = active.iter().map(|s| s.name.clone()).collect();
        let (p0, p1) = if names.len() == 2 {
            (names[0].clone(), names[1].clone())
        } else if let Some(agg) = aggressor {
            let pos = names.iter().position(|n| *n == agg).unwrap_or(0);
            let next = names[(pos + 1) % names.len()].clone();
            (agg, next)
        } else {
            (names[0].clone(), names[1].clone())
        };
        if names.len() > 2 {
            warnings.push(format!(
                "{} players active on the {}; this solver is heads-up, using {} vs {} and folding the rest into the pot",
                names.len(),
                street.name,
                p0,
                p1
            ));
        }
        let oop = if self.order_key(
            active
                .iter()
                .find(|s| s.name == p0)
                .map(|s| s.number)
                .unwrap_or(0),
        ) <= self.order_key(
            active
                .iter()
                .find(|s| s.name == p1)
                .map(|s| s.number)
                .unwrap_or(0),
        ) {
            p0.clone()
        } else {
            p1.clone()
        };
        let ip = if oop == p0 { p1.clone() } else { p0.clone() };
        let position_of = |name: &str| -> String {
            active
                .iter()
                .find(|s| s.name == name)
                .map(|s| self.position_label(s.number))
                .unwrap_or_default()
        };
        let oop_position = position_of(&oop);
        let ip_position = position_of(&ip);

        if preflop.raises == 0 {
            notes.push("limped pot: there is no preflop raiser; use limped/checking ranges".into());
        } else if preflop.raises >= 2 {
            if let (Some(o), Some(a)) = (&preflop.opener, &preflop.last_aggressor) {
                if o.name != a.name {
                    notes.push(format!(
                        "{}: original raiser {} vs preflop aggressor {}",
                        preflop.pot_type.short(),
                        o.label(),
                        a.label()
                    ));
                }
            }
        }
        if let Some(o) = &preflop.opener {
            if !active.iter().any(|s| s.name == o.name) {
                warnings.push(format!(
                    "preflop opener {} folded before the {}; the range of the remaining player is not an opener range",
                    o.label(),
                    street.name
                ));
            }
        }
        if let Some(a) = &preflop.last_aggressor {
            let differs_from_opener = preflop
                .opener
                .as_ref()
                .map(|o| o.name != a.name)
                .unwrap_or(true);
            if differs_from_opener && !active.iter().any(|s| s.name == a.name) {
                warnings.push(format!(
                    "preflop aggressor {} folded before the {}; no preflop aggressor is in the solved pair",
                    a.label(),
                    street.name
                ));
            }
        }
        if preflop.raises > 0 {
            let actor_in_pair = |p: &Option<PreflopPlayer>| {
                p.as_ref()
                    .map(|p| p.name.as_str() == oop.as_str() || p.name.as_str() == ip.as_str())
                    .unwrap_or(false)
            };
            if !actor_in_pair(&preflop.opener) && !actor_in_pair(&preflop.last_aggressor) {
                notes.push(
                    "the solved heads-up pair does not contain the preflop opener or aggressor: preflop roles refer to the full hand"
                        .into(),
                );
            }
        }

        // Effective stack behind at the analysed street.
        let stack_of = |name: &str| -> f64 {
            let seat = self.seats.iter().find(|s| s.name == name);
            let start = seat.map(|s| s.stack).unwrap_or(0.0);
            (start - contrib.get(name).copied().unwrap_or(0.0)).max(0.0)
        };
        let stack_chips = stack_of(&oop).min(stack_of(&ip));
        if stack_chips <= 0.0 {
            warnings.push("a player is already all-in: stack behind is zero".into());
        }

        let mut sizes = street_sizes[idx].clone();
        sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sizes.dedup();
        if sizes.is_empty() {
            sizes.push(0.75);
        }

        if let Some(h) = &self.hero {
            if !active.iter().any(|s| &s.name == h) {
                warnings.push(format!(
                    "hero {h} is not in the hand on the {}: hole cards are shown for reference only",
                    street.name
                ));
            }
        }
        if let Some(small) = sizes.iter().find(|x| **x < 0.05) {
            notes.push(format!(
                "observed bet is {:.0}% of the pot; add a second size (e.g. 0.75) for a useful tree",
                small * 100.0
            ));
        }
        if self.rake > 0.0 {
            notes.push(format!(
                "rake {:.2} on a {:.2} pot ({:.1}%): solver rake set to {:.1}%",
                self.rake,
                self.total_pot,
                if self.total_pot > 0.0 {
                    self.rake / self.total_pot * 100.0
                } else {
                    0.0
                },
                if self.total_pot > 0.0 {
                    self.rake / self.total_pot * 100.0
                } else {
                    0.0
                }
            ));
        } else {
            notes.push(
                "no rake recorded in this hand (rounding at micro stakes): solver rake left as configured"
                    .into(),
            );
        }

        Ok(Spot {
            street: street.name.clone(),
            board: street.cards.clone(),
            pot_bb: pot_chips / bb,
            stack_bb: stack_chips / bb,
            pot_chips,
            stack_chips,
            oop,
            ip,
            oop_position,
            ip_position,
            preflop,
            bet_sizes: sizes,
            hero: self.hero.clone(),
            hero_cards: self.hero_cards.clone(),
            sb: self.sb,
            bb,
            rake: self.rake,
            total_pot: self.total_pot,
            rake_pct_observed: if self.total_pot > 0.0 {
                self.rake / self.total_pot
            } else {
                0.0
            },
            notes,
            warnings,
        })
    }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

pub fn spot_summary(spot: &Spot) -> String {
    let board = spot
        .board
        .iter()
        .map(|c| card_str(*c))
        .collect::<Vec<_>>()
        .join(" ");
    let mut s = String::new();
    s.push_str(&format!("street       {} ({board})\n", spot.street));
    s.push_str(&format!(
        "blinds       {:.2}/{:.2}\n",
        spot.sb, spot.bb
    ));
    s.push_str(&format!(
        "pot          {:.2} chips = {:.2} bb\n",
        spot.pot_chips, spot.pot_bb
    ));
    s.push_str(&format!(
        "stack        {:.2} chips = {:.2} bb\n",
        spot.stack_chips, spot.stack_bb
    ));
    s.push_str(&format!("players      OOP {} / IP {}\n", spot.oop, spot.ip));
    s.push_str(&format!(
        "positions    OOP {} / IP {}\n",
        spot.oop_position, spot.ip_position
    ));
    s.push_str(&format!(
        "preflop      {}\n",
        spot.preflop.summary()
    ));
    s.push_str(&format!("roles        {}\n", spot.preflop.roles()));
    s.push_str(&format!(
        "rake         {:.2} on a {:.2} pot ({:.1}%)\n",
        spot.rake,
        spot.total_pot,
        spot.rake_pct_observed * 100.0
    ));
    s.push_str(&format!(
        "sizes        {}\n",
        spot.bet_sizes
            .iter()
            .map(|x| format!("{:.0}% pot", x * 100.0))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if let Some(h) = &spot.hero {
        let cards = spot
            .hero_cards
            .iter()
            .map(|c| card_str(*c))
            .collect::<Vec<_>>()
            .join("");
        s.push_str(&format!("hero         {h} [{cards}]\n"));
    }
    for n in &spot.notes {
        s.push_str(&format!("note         {n}\n"));
    }
    for w in &spot.warnings {
        s.push_str(&format!("warning      {w}\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"PokerStars Hand #262425598136:  Hold'em No Limit (€0.01/€0.02 EUR) [ADM ID: M66B23049E4FB4GU] - 2026/10/17 14:06:50 UTC [2026/10/17 10:06:50 ET]
Table 'Sundmania' 6-max Seat #6 is the button
Seat 5: moriglia49 (€1.29 in chips) is sitting out
Seat 1: Edopay007 (€3.09 in chips)
Seat 2: supSharp (€2 in chips)
Seat 3: Pekoz051 (€2 in chips)
Seat 4: SkyAA1 (€1.83 in chips)
Seat 6: lucianaxxxx (€1.29 in chips)
Edopay007: posts small blind €0.01
supSharp: posts big blind €0.02
moriglia49: sits out
*** HOLE CARDS ***
Dealt to Pekoz051 [3c Kd]
Pekoz051: folds
SkyAA1: calls €0.02
lucianaxxxx: calls €0.02
Edopay007: folds
supSharp: checks
*** FLOP *** [Ah 7h 7s]
supSharp: checks
SkyAA1: checks
lucianaxxxx: checks
*** TURN *** [Ah 7h 7s] [9c]
supSharp: checks
SkyAA1: checks
lucianaxxxx: bets €0.04
supSharp: folds
SkyAA1: folds
Uncalled bet (€0.04) returned to lucianaxxxx
lucianaxxxx collected €0.07 from pot
lucianaxxxx: doesn't show hand
moriglia49 leaves the table
*** SUMMARY ***
Total pot €0.07 | Rake €0
Board [Ah 7h 7s 9c]
Seat 5: moriglia49
Seat 1: Edopay007 (small blind) folded before Flop
Seat 2: supSharp (big blind) folded on the Turn
Seat 3: Pekoz051 folded before Flop (didn't bet)
Seat 4: SkyAA1 folded on the Turn
Seat 6: lucianaxxxx (button) collected (€0.07) "#;

    #[test]
    fn parses_sample_hand() {
        let h = parse(SAMPLE).unwrap();
        assert_eq!(h.sb, 0.01);
        assert_eq!(h.bb, 0.02);
        assert_eq!(h.button, 6);
        assert_eq!(h.seats.len(), 6);
        assert_eq!(h.hero.as_deref(), Some("Pekoz051"));
        assert_eq!(h.hero_cards.len(), 2);
        assert_eq!(h.streets.len(), 3); // preflop, flop, turn
        assert_eq!(h.total_pot, 0.07);
    }

    #[test]
    fn derives_turn_spot() {
        let h = parse(SAMPLE).unwrap();
        let s = h.spot().unwrap();
        assert_eq!(s.street, "turn");
        assert_eq!(s.board.len(), 4);
        assert!((s.pot_chips - 0.07).abs() < 1e-9);
        assert!((s.pot_bb - 3.5).abs() < 1e-9);
        // lucianaxxxx has 1.27 behind, supSharp 1.98 -> effective 63.5 bb.
        assert!((s.stack_bb - 63.5).abs() < 1e-6, "stack {}", s.stack_bb);
        assert_eq!(s.oop, "supSharp");
        assert_eq!(s.ip, "lucianaxxxx");
        assert_eq!(s.bet_sizes, vec![0.57]);
        assert!(!s.warnings.is_empty()); // multiway warning
    }

    #[test]
    fn derives_river_spot_with_rake() {
        let text = r#"PokerStars Hand #262425592655:  Hold'em No Limit (€0.01/€0.02 EUR) - 2026/10/17 14:06:12 UTC
Table 'Moreau' 6-max Seat #1 is the button
Seat 1: Pekoz051 (€2 in chips)
Seat 2: jurhas1 (€0.07 in chips)
Seat 3: cobra.it57 (€1.52 in chips)
Seat 4: Miluzzo2026 (€2.03 in chips)
Seat 5: miseri90 (€0.99 in chips)
Seat 6: Natural_Wisdom (€1.66 in chips)
jurhas1: posts small blind €0.01
cobra.it57: posts big blind €0.02
*** HOLE CARDS ***
Dealt to Pekoz051 [As Ac]
Miluzzo2026: folds
miseri90: calls €0.02
Natural_Wisdom: calls €0.02
Pekoz051: raises €0.08 to €0.10
jurhas1: folds
cobra.it57: calls €0.08
miseri90: calls €0.08
Natural_Wisdom: folds
*** FLOP *** [Qs 2d Th]
cobra.it57: bets €0.02
miseri90: calls €0.02
Pekoz051: raises €0.20 to €0.22
cobra.it57: calls €0.20
miseri90: calls €0.20
*** TURN *** [Qs 2d Th] [Td]
cobra.it57: bets €0.02
miseri90: folds
Pekoz051: raises €0.26 to €0.28
cobra.it57: raises €0.26 to €0.54
Pekoz051: calls €0.26
*** RIVER *** [Qs 2d Th Td] [6c]
cobra.it57: bets €0.02
Pekoz051: calls €0.02
*** SUMMARY ***
Total pot €2.11 | Rake €0.12
Board [Qs 2d Th Td 6c]
"#;
        let h = parse(text).unwrap();
        let s = h.spot().unwrap();
        assert_eq!(s.street, "river");
        assert_eq!(s.board.len(), 5);
        assert!((s.pot_chips - 2.07).abs() < 1e-9, "pot {}", s.pot_chips);
        assert!((s.stack_chips - 0.64).abs() < 1e-9, "stack {}", s.stack_chips);
        assert_eq!(s.oop, "cobra.it57");
        assert_eq!(s.ip, "Pekoz051");
        assert_eq!(s.bet_sizes, vec![0.01]);
        assert!((s.rake - 0.12).abs() < 1e-9);
        assert!(s.notes.iter().any(|n| n.contains("5.7%")), "{:?}", s.notes);
        assert!(s.warnings.is_empty());
    }

    #[test]
    fn rejects_non_pokerstars_text() {
        assert!(parse("hello world").is_err());
    }

    #[test]
    fn labels_positions_from_the_button() {
        let h = parse(SAMPLE).unwrap();
        assert_eq!(h.position_label(6), "BTN");
        assert_eq!(h.position_label(1), "SB");
        assert_eq!(h.position_label(2), "BB");
        assert_eq!(h.position_label(3), "UTG");
        assert_eq!(h.position_label(4), "HJ");
        assert_eq!(h.position_label(5), "CO");
    }

    #[test]
    fn classifies_limped_pot() {
        let h = parse(SAMPLE).unwrap();
        let line = h.preflop_line();
        assert_eq!(line.pot_type, PotType::Limped);
        assert_eq!(line.raises, 0);
        assert!(line.opener.is_none());
        assert!(line.last_aggressor.is_none());
        let s = h.spot().unwrap();
        assert_eq!(s.preflop.pot_type, PotType::Limped);
        assert_eq!(s.oop_position, "BB");
        assert_eq!(s.ip_position, "BTN");
        assert!(s.notes.iter().any(|n| n.contains("limped pot")));
    }

    #[test]
    fn classifies_srp_and_open_size() {
        let text = r#"PokerStars Hand #262425592655:  Hold'em No Limit (€0.01/€0.02 EUR) - 2026/10/17 14:06:12 UTC
Table 'Moreau' 6-max Seat #1 is the button
Seat 1: Pekoz051 (€2 in chips)
Seat 2: jurhas1 (€0.07 in chips)
Seat 3: cobra.it57 (€1.52 in chips)
Seat 4: Miluzzo2026 (€2.03 in chips)
Seat 5: miseri90 (€0.99 in chips)
Seat 6: Natural_Wisdom (€1.66 in chips)
jurhas1: posts small blind €0.01
cobra.it57: posts big blind €0.02
*** HOLE CARDS ***
Dealt to Pekoz051 [As Ac]
Miluzzo2026: folds
miseri90: calls €0.02
Natural_Wisdom: calls €0.02
Pekoz051: raises €0.08 to €0.10
jurhas1: folds
cobra.it57: calls €0.08
miseri90: calls €0.08
Natural_Wisdom: folds
*** FLOP *** [Qs 2d Th]
cobra.it57: bets €0.02
miseri90: calls €0.02
Pekoz051: raises €0.20 to €0.22
cobra.it57: calls €0.20
miseri90: calls €0.20
*** SUMMARY ***
Total pot €2.11 | Rake €0.12
Board [Qs 2d Th]
"#;
        let h = parse(text).unwrap();
        let line = h.preflop_line();
        assert_eq!(line.pot_type, PotType::SingleRaised);
        assert_eq!(line.raises, 1);
        assert_eq!(line.opener.as_ref().unwrap().name, "Pekoz051");
        assert_eq!(line.opener.as_ref().unwrap().position, "BTN");
        assert_eq!(line.last_aggressor.as_ref().unwrap().name, "Pekoz051");
        assert!((line.open_to.unwrap() - 0.10).abs() < 1e-9);
        assert!((line.open_to_bb.unwrap() - 5.0).abs() < 1e-9);
        assert_eq!(line.callers.len(), 2);
        let s = h.spot().unwrap();
        assert_eq!(s.oop_position, "BB");
        assert_eq!(s.ip_position, "BTN");
        assert_eq!(s.preflop.summary(), "BTN opens 5.00 bb, BB/HJ calls -> SRP");
    }

    #[test]
    fn classifies_three_bet_and_keeps_opener_separate() {
        let text = r#"PokerStars Hand #3:  Hold'em No Limit ($0.50/$1 USD) - 2026/10/18 10:00:00 UTC
Table 'Test' 6-max Seat #1 is the button
Seat 1: BTN ($100 in chips)
Seat 2: SB ($100 in chips)
Seat 3: BB ($100 in chips)
Seat 4: UTG ($100 in chips)
Seat 5: HJ ($100 in chips)
Seat 6: CO ($100 in chips)
SB: posts small blind $0.50
BB: posts big blind $1
*** HOLE CARDS ***
UTG: folds
HJ: folds
CO: raises $1.50 to $2.50
BTN: raises $5.50 to $8
SB: folds
BB: folds
CO: calls $5.50
*** FLOP *** [Ah Kd 7c]
CO: checks
BTN: bets $5
CO: folds
*** SUMMARY ***
Total pot $17.50 | Rake $0
Board [Ah Kd 7c]
"#;
        let h = parse(text).unwrap();
        let line = h.preflop_line();
        assert_eq!(line.pot_type, PotType::ThreeBet);
        assert_eq!(line.raises, 2);
        assert_eq!(line.opener.as_ref().unwrap().name, "CO");
        assert_eq!(line.last_aggressor.as_ref().unwrap().name, "BTN");
        let s = h.spot().unwrap();
        assert_eq!(s.oop_position, "CO");
        assert_eq!(s.ip_position, "BTN");
        assert_eq!(s.preflop.summary(), "CO opens 2.50 bb, BTN 3-bets -> 3BP");
        assert!(s
            .notes
            .iter()
            .any(|n| n.contains("original raiser CO") && n.contains("preflop aggressor BTN")));
    }

    #[test]
    fn warns_when_opener_folded_before_the_spot() {
        let text = r#"PokerStars Hand #4:  Hold'em No Limit ($0.50/$1 USD) - 2026/10/18 10:00:00 UTC
Table 'Test' 6-max Seat #1 is the button
Seat 1: BTN ($100 in chips)
Seat 2: SB ($100 in chips)
Seat 3: BB ($100 in chips)
Seat 4: UTG ($100 in chips)
Seat 5: HJ ($100 in chips)
Seat 6: CO ($100 in chips)
SB: posts small blind $0.50
BB: posts big blind $1
*** HOLE CARDS ***
UTG: folds
HJ: folds
CO: raises $1.50 to $2.50
BTN: raises $5.50 to $8
SB: calls $7.50
BB: folds
CO: folds
*** FLOP *** [Ah Kd 7c]
SB: checks
BTN: bets $5
SB: folds
*** SUMMARY ***
Total pot $17.50 | Rake $0
Board [Ah Kd 7c]
"#;
        let h = parse(text).unwrap();
        let s = h.spot().unwrap();
        assert_eq!(s.preflop.pot_type, PotType::ThreeBet);
        assert_eq!(s.oop_position, "SB");
        assert_eq!(s.ip_position, "BTN");
        assert!(s
            .warnings
            .iter()
            .any(|w| w.contains("preflop opener CO") && w.contains("folded before the flop")));
        assert!(crate::presets::suggest_ranges(&s).is_none());
    }
}
