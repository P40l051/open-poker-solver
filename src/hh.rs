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
    fn order_key(&self, seat: u32) -> i64 {
        let n = self.seats.len().max(2) as i64;
        (seat as i64 - self.button as i64 - 1).rem_euclid(n)
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
        let mut notes: Vec<String> = Vec::new();
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
}
