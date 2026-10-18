//! Starter preflop ranges for the imported spot.
//!
//! These are deliberately labelled as a heuristic chart, not a solved GTO
//! range. They give the user a sane starting point after importing a hand; the
//! ranges remain editable and the solver always uses exactly what it is given.

use crate::hh::{PotType, Spot};

#[derive(Clone, Debug, PartialEq)]
pub struct RangeSuggestion {
    pub oop_range: String,
    pub ip_range: String,
    pub source: String,
    pub notes: Vec<String>,
}

const SOURCE: &str = "OPS starter chart (heuristic, not a solved range)";

// ---------------------------------------------------------------------------
// Open / iso-raise ranges, 6-max cash, 100 bb.
// ---------------------------------------------------------------------------
const OPEN_UTG: &str = "22+,A2s+,KTs+,QTs+,JTs,T9s,98s,87s,76s,ATo+,KQo";
const OPEN_HJ: &str = "22+,A2s+,K9s+,Q9s+,J9s+,T8s+,97s+,86s+,75s+,65s,ATo+,KJo+,QJo";
const OPEN_CO: &str = "22+,A2s+,K7s+,Q8s+,J8s+,T7s+,96s+,85s+,74s+,64s+,54s,A9o+,KTo+,QTo+,JTo";
const OPEN_BTN: &str = "22+,A2s+,K2s+,Q4s+,J6s+,T6s+,95s+,85s+,74s+,63s+,53s+,43s,A2o+,K7o+,Q8o+,J8o+,T8o+,98o,87o";
const OPEN_SB: &str = "22+,A2s+,K2s+,Q3s+,J5s+,T6s+,95s+,84s+,74s+,63s+,53s+,43s,A2o+,K8o+,Q9o+,J9o+,T9o";
const OPEN_BB: &str = "22+,A2s+,K9s+,Q9s+,J9s+,T8s+,97s+,86s+,75s+,65s,ATo+,KJo+";

// ---------------------------------------------------------------------------
// Calls against a single open.
// ---------------------------------------------------------------------------
const BB_VS_EARLY: &str = "22+,A2s+,K9s+,Q9s+,J9s+,T8s+,97s+,86s+,75s+,65s,ATo+,KJo+,QJo";
const BB_VS_LATE: &str = "22+,A2s+,K2s+,Q4s+,J6s+,T6s+,95s+,85s+,74s+,63s+,53s+,43s,A2o+,K7o+,Q8o+,J8o+,T8o+,98o,87o";
const SB_VS_EARLY: &str = "22+,A2s+,K9s+,QTs+,JTs,T9s,98s,AJo+,KQo";
const SB_VS_LATE: &str = "22+,A2s+,K7s+,Q8s+,J8s+,T7s+,96s+,85s+,75s+,65s,A9o+,KTo+,QTo+";
const IP_CALL_EARLY: &str = "TT-55,AQs-A9s,KQs,KJs,QJs,JTs,T9s,98s,AQo";
const IP_CALL_LATE: &str = "22+,A2s+,K9s+,Q9s+,J9s+,T8s+,97s+,86s+,75s+,65s,ATo+,KJo+";

// ---------------------------------------------------------------------------
// 3-bet and continue-vs-3-bet ranges.
// ---------------------------------------------------------------------------
const THREEBET_IP_VS_EARLY: &str = "QQ+,AKs,AKo,A5s-A2s,KQs";
const THREEBET_IP_VS_LATE: &str = "TT+,AQs+,AKo,A5s-A4s,KQs";
const THREEBET_BLIND_VS_EARLY: &str = "QQ+,AKs,AKo,A5s-A3s";
const THREEBET_BLIND_VS_LATE: &str = "TT+,AQs+,AKo,A5s-A3s,KQs,QJs";
const CALL_3BET_EARLY: &str = "TT-77,AQs+,KQs,AKo";
const CALL_3BET_LATE: &str = "TT-66,AQs+,KQs,AQo,AKo,A5s";

// ---------------------------------------------------------------------------
// 4-bet and continue-vs-4-bet ranges, plus limped pots.
// ---------------------------------------------------------------------------
const FOURBET: &str = "QQ+,AKs,AKo,A5s";
const CALL_4BET: &str = "JJ+,AQs+,AKo";
const LIMP: &str = "22+,A2s+,K7s+,Q8s+,J8s+,T7s+,96s+,85s+,74s+,64s+,54s,A9o+,KTo+,QTo+,JTo";

fn early(pos: &str) -> bool {
    matches!(
        pos,
        "UTG" | "UTG+1" | "UTG+2" | "MP" | "MP+1" | "HJ"
    )
}

fn blind(pos: &str) -> bool {
    matches!(pos, "SB" | "BB")
}

fn opener_range(pos: &str) -> Option<&'static str> {
    match pos {
        "UTG" | "UTG+1" | "UTG+2" | "MP" | "MP+1" => Some(OPEN_UTG),
        "HJ" => Some(OPEN_HJ),
        "CO" => Some(OPEN_CO),
        "BTN" => Some(OPEN_BTN),
        "SB" => Some(OPEN_SB),
        "BB" => Some(OPEN_BB),
        _ => None,
    }
}

fn caller_range(caller: &str, opener: &str) -> Option<&'static str> {
    match caller {
        "BB" => Some(if early(opener) { BB_VS_EARLY } else { BB_VS_LATE }),
        "SB" => Some(if early(opener) { SB_VS_EARLY } else { SB_VS_LATE }),
        _ => Some(if early(opener) { IP_CALL_EARLY } else { IP_CALL_LATE }),
    }
}

fn threebet_range(aggressor: &str, opener: &str) -> Option<&'static str> {
    if blind(aggressor) {
        Some(if early(opener) {
            THREEBET_BLIND_VS_EARLY
        } else {
            THREEBET_BLIND_VS_LATE
        })
    } else {
        Some(if early(opener) {
            THREEBET_IP_VS_EARLY
        } else {
            THREEBET_IP_VS_LATE
        })
    }
}

fn call_3bet_range(opener: &str) -> Option<&'static str> {
    Some(if early(opener) {
        CALL_3BET_EARLY
    } else {
        CALL_3BET_LATE
    })
}

fn assign(spot: &Spot, aggressor: &str, aggressor_range: &str, other_range: &str) -> (String, String) {
    if spot.oop == aggressor {
        (aggressor_range.to_string(), other_range.to_string())
    } else {
        (other_range.to_string(), aggressor_range.to_string())
    }
}

fn other_name<'a>(spot: &'a Spot, name: &str) -> Option<&'a str> {
    if spot.oop == name {
        Some(spot.ip.as_str())
    } else if spot.ip == name {
        Some(spot.oop.as_str())
    } else {
        None
    }
}

fn other_position<'a>(spot: &'a Spot, name: &str) -> Option<&'a str> {
    if spot.oop == name {
        Some(spot.ip_position.as_str())
    } else if spot.ip == name {
        Some(spot.oop_position.as_str())
    } else {
        None
    }
}

/// Returns starter ranges for the two players in the solved spot.
///
/// `None` means the preflop actors are not both part of the heads-up pair (for
/// example the opener folded, or a multiway hand was reduced to two callers).
pub fn suggest_ranges(spot: &Spot) -> Option<RangeSuggestion> {
    let pl = &spot.preflop;
    let mut notes = vec!["Starter chart only: edit the ranges before solving.".to_string()];

    match pl.pot_type {
        PotType::Limped => {
            let oop_range = if spot.oop_position == "BB" {
                "random"
            } else {
                LIMP
            };
            let ip_range = if spot.ip_position == "BB" {
                "random"
            } else {
                LIMP
            };
            notes.push(
                "Limped pot: ranges are wide and very approximate; the big blind checks any two."
                    .into(),
            );
            Some(RangeSuggestion {
                oop_range: oop_range.to_string(),
                ip_range: ip_range.to_string(),
                source: SOURCE.into(),
                notes,
            })
        }
        PotType::SingleRaised => {
            let opener = pl.opener.as_ref()?;
            let other = other_name(spot, &opener.name)?;
            let other_pos = other_position(spot, &opener.name)?;
            let opener_range = opener_range(&opener.position)?;
            let caller_range = caller_range(other_pos, &opener.position)?;
            if !pl.callers.iter().any(|c| c.name == other) {
                notes.push(
                    "The other player did not call the open in the parsed preflop line: ranges are approximate."
                        .into(),
                );
            }
            let (oop_range, ip_range) =
                assign(spot, &opener.name, opener_range, caller_range);
            Some(RangeSuggestion {
                oop_range,
                ip_range,
                source: SOURCE.into(),
                notes,
            })
        }
        PotType::ThreeBet => {
            let opener = pl.opener.as_ref()?;
            let aggressor = pl.last_aggressor.as_ref()?;
            if opener.name == aggressor.name {
                return None;
            }
            other_name(spot, &opener.name)?;
            other_name(spot, &aggressor.name)?;
            let agg_range = threebet_range(&aggressor.position, &opener.position)?;
            let opener_range = call_3bet_range(&opener.position)?;
            let (oop_range, ip_range) =
                assign(spot, &aggressor.name, agg_range, opener_range);
            Some(RangeSuggestion {
                oop_range,
                ip_range,
                source: SOURCE.into(),
                notes,
            })
        }
        PotType::FourBetPlus => {
            let opener = pl.opener.as_ref()?;
            let aggressor = pl.last_aggressor.as_ref()?;
            if opener.name == aggressor.name {
                return None;
            }
            other_name(spot, &opener.name)?;
            other_name(spot, &aggressor.name)?;
            let (oop_range, ip_range) =
                assign(spot, &aggressor.name, FOURBET, CALL_4BET);
            notes.push(
                "4-bet+ pot: starter ranges are a broad approximation of a solved 4-bet range."
                    .into(),
            );
            Some(RangeSuggestion {
                oop_range,
                ip_range,
                source: SOURCE.into(),
                notes,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hh;
    use crate::range::Range;

    const SRP: &str = r#"PokerStars Hand #1:  Hold'em No Limit ($0.50/$1 USD) - 2026/10/18 10:00:00 UTC
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
CO: folds
BTN: raises $1.50 to $2.50
SB: folds
BB: calls $1.50
*** FLOP *** [Ah Kd 7c]
BB: checks
BTN: bets $2
BB: folds
*** SUMMARY ***
Total pot $9 | Rake $0
Board [Ah Kd 7c]
"#;

    const THREE_BET: &str = r#"PokerStars Hand #2:  Hold'em No Limit ($0.50/$1 USD) - 2026/10/18 10:00:00 UTC
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

    fn spot(text: &str) -> Spot {
        hh::parse(text).unwrap().spot().unwrap()
    }

    #[test]
    fn suggests_srp_opener_and_blind_caller() {
        let s = spot(SRP);
        assert_eq!(s.preflop.pot_type, PotType::SingleRaised);
        assert_eq!(s.preflop.opener.as_ref().unwrap().position, "BTN");
        assert_eq!(s.oop_position, "BB");
        assert_eq!(s.ip_position, "BTN");
        let r = suggest_ranges(&s).unwrap();
        assert_eq!(r.oop_range, BB_VS_LATE);
        assert_eq!(r.ip_range, OPEN_BTN);
        Range::from_str(&r.oop_range).unwrap();
        Range::from_str(&r.ip_range).unwrap();
    }

    #[test]
    fn suggests_three_bet_roles() {
        let s = spot(THREE_BET);
        assert_eq!(s.preflop.pot_type, PotType::ThreeBet);
        assert_eq!(s.preflop.opener.as_ref().unwrap().position, "CO");
        assert_eq!(s.preflop.last_aggressor.as_ref().unwrap().position, "BTN");
        assert_eq!(s.oop_position, "CO");
        assert_eq!(s.ip_position, "BTN");
        let r = suggest_ranges(&s).unwrap();
        assert_eq!(r.oop_range, CALL_3BET_LATE);
        assert_eq!(r.ip_range, THREEBET_IP_VS_LATE);
        Range::from_str(&r.oop_range).unwrap();
        Range::from_str(&r.ip_range).unwrap();
    }

    #[test]
    fn all_starter_ranges_parse() {
        for r in [
            OPEN_UTG,
            OPEN_HJ,
            OPEN_CO,
            OPEN_BTN,
            OPEN_SB,
            OPEN_BB,
            BB_VS_EARLY,
            BB_VS_LATE,
            SB_VS_EARLY,
            SB_VS_LATE,
            IP_CALL_EARLY,
            IP_CALL_LATE,
            THREEBET_IP_VS_EARLY,
            THREEBET_IP_VS_LATE,
            THREEBET_BLIND_VS_EARLY,
            THREEBET_BLIND_VS_LATE,
            CALL_3BET_EARLY,
            CALL_3BET_LATE,
            FOURBET,
            CALL_4BET,
            LIMP,
        ] {
            Range::from_str(r).unwrap_or_else(|e| panic!("{r}: {e}"));
        }
    }
}
