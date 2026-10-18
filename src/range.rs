//! Range parsing and combo weighting.
//!
//! Accepts the usual shorthand (`QQ+`, `A2s+`, `T9s-76s`, `AKo`, `random`)
//! plus optional frequencies (`A5s:0.5`). Every range is stored as a dense
//! `[f32; 1326]` weight vector, `0.0..=1.0` per combo.

use crate::cards::{combo_index, rank, NCOMBOS};

#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    pub weights: Vec<f32>,
}

impl Default for Range {
    fn default() -> Self {
        Self {
            weights: vec![1.0; NCOMBOS],
        }
    }
}

impl Range {
    pub fn empty() -> Self {
        Self {
            weights: vec![0.0; NCOMBOS],
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        let mut r = Range::empty();
        let mut any = false;
        for raw in s.split(',') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let (core, weight) = match token.rsplit_once(':') {
                Some((c, w)) => {
                    let w: f32 = w
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid weight in {token:?}"))?;
                    if !(0.0..=1.0).contains(&w) {
                        return Err(format!("weight out of [0,1] in {token:?}"));
                    }
                    (c.trim(), w)
                }
                None => (token, 1.0),
            };
            match core.to_ascii_lowercase().as_str() {
                "random" | "any" | "*" | "100%" => {
                    any = true;
                    continue;
                }
                _ => {}
            }
            for idx in expand(core)? {
                r.weights[idx] = r.weights[idx].max(weight);
            }
        }
        if any {
            for w in r.weights.iter_mut() {
                *w = w.max(1.0);
            }
        }
        if r.total() <= 0.0 {
            return Err(format!("range {s:?} is empty"));
        }
        Ok(r)
    }

    pub fn total(&self) -> f64 {
        self.weights.iter().map(|w| *w as f64).sum()
    }

    /// Mass in combos (unweighted count would be the combo count).
    pub fn combos(&self) -> usize {
        self.weights.iter().filter(|w| **w > 0.0).count()
    }

    /// Fraction of all 1326 combos, weighted.
    pub fn percent(&self) -> f64 {
        self.total() / NCOMBOS as f64 * 100.0
    }

    /// Normalizes the vector to sum to 1, returning it.
    pub fn normalized(&self) -> Vec<f32> {
        let t = self.total();
        if t <= 0.0 {
            return vec![0.0; NCOMBOS];
        }
        self.weights.iter().map(|w| (*w as f64 / t) as f32).collect()
    }
}

fn rank_index(ch: char) -> Result<u8, String> {
    match ch.to_ascii_uppercase() {
        '2' => Ok(0),
        '3' => Ok(1),
        '4' => Ok(2),
        '5' => Ok(3),
        '6' => Ok(4),
        '7' => Ok(5),
        '8' => Ok(6),
        '9' => Ok(7),
        'T' => Ok(8),
        'J' => Ok(9),
        'Q' => Ok(10),
        'K' => Ok(11),
        'A' => Ok(12),
        _ => Err(format!("invalid rank {ch:?}")),
    }
}

/// Collects combos for one hand class.
fn class(hi: u8, lo: u8, suitedness: Option<bool>, out: &mut Vec<usize>) {
    debug_assert!(hi >= lo);
    if hi == lo {
        for s1 in 0..4u8 {
            for s2 in (s1 + 1)..4 {
                out.push(combo_index(hi * 4 + s1, lo * 4 + s2));
            }
        }
        return;
    }
    // hi and lo are rank indices; hi card is the higher rank so must be the
    // smaller card id for combo_index.
    for s1 in 0..4u8 {
        for s2 in 0..4u8 {
            let suited = s1 == s2;
            let want = match suitedness {
                None => true,
                Some(w) => w == suited,
            };
            if want {
                out.push(combo_index(lo * 4 + s2, hi * 4 + s1));
            }
        }
    }
}

fn expand(token: &str) -> Result<Vec<usize>, String> {
    let t = token.trim();
    let (body, plus) = match t.strip_suffix('+') {
        Some(b) => (b, true),
        None => (t, false),
    };

    if let Some((left, right)) = body.split_once('-') {
        return expand_dash(left, right);
    }

    let bytes = body.as_bytes();
    if bytes.len() < 2 {
        return Err(format!("invalid hand {token:?}"));
    }
    let hi = rank_index(bytes[0] as char)?;
    let lo = rank_index(bytes[1] as char)?;
    let suitedness = match bytes.get(2).map(|b| b.to_ascii_lowercase()) {
        None => None,
        Some(b's') => Some(true),
        Some(b'o') => Some(false),
        Some(other) => return Err(format!("invalid suffix {:?} in {token:?}", other as char)),
    };
    if bytes.len() > 3 {
        return Err(format!("invalid hand {token:?}"));
    }

    let mut out = Vec::new();
    if plus {
        if hi == lo {
            for r in hi..=12 {
                class(r, r, None, &mut out);
            }
        } else if hi == 12 {
            // A2s+ .. AKs keeps the ace fixed and climbs with the kicker.
            for r in lo..=11 {
                class(12, r, suitedness, &mut out);
            }
        } else {
            // T9s+ -> T9s, JTs, QJs, KQs, AKs (constant gap).
            let gap = hi - lo;
            let mut h = hi;
            while h <= 12 {
                class(h, h - gap, suitedness, &mut out);
                h += 1;
            }
        }
    } else {
        let (h, l) = if hi >= lo { (hi, lo) } else { (lo, hi) };
        class(h, l, suitedness, &mut out);
    }
    Ok(out)
}

fn expand_dash(left: &str, right: &str) -> Result<Vec<usize>, String> {
    let parse = |s: &str| -> Result<(u8, u8, Option<bool>), String> {
        let b = s.as_bytes();
        if b.len() < 2 {
            return Err(format!("invalid hand {s:?}"));
        }
        let hi = rank_index(b[0] as char)?;
        let lo = rank_index(b[1] as char)?;
        let suitedness = match b.get(2).map(|x| x.to_ascii_lowercase()) {
            None => None,
            Some(b's') => Some(true),
            Some(b'o') => Some(false),
            Some(_) => return Err(format!("invalid hand {s:?}")),
        };
        Ok((hi.max(lo), hi.min(lo), suitedness))
    };
    let (h1, l1, s1) = parse(left)?;
    let (h2, l2, s2) = parse(right)?;
    if s1 != s2 {
        return Err(format!("mismatched suits in range {left}-{right}"));
    }
    let mut out = Vec::new();
    if h1 == l1 && h2 == l2 {
        let (a, b) = (h1.min(h2), h1.max(h2));
        for r in a..=b {
            class(r, r, None, &mut out);
        }
    } else if h1 == h2 {
        let (a, b) = (l1.min(l2), l1.max(l2));
        for r in a..=b {
            class(h1, r, s1, &mut out);
        }
    } else if h1 - l1 == h2 - l2 {
        let gap = h1 - l1;
        let (a, b) = (h1.min(h2), h1.max(h2));
        for h in a..=b {
            class(h, h - gap, s1, &mut out);
        }
    } else {
        return Err(format!("unsupported range {left}-{right}"));
    }
    Ok(out)
}

/// Compact human summary of the top hand classes in a weight vector.
pub fn top_classes(weights: &[f32], min_weight: f32, limit: usize) -> Vec<(String, f32, usize)> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, (f32, usize)> = BTreeMap::new();
    for (idx, w) in weights.iter().enumerate() {
        if *w < min_weight {
            continue;
        }
        let (a, b) = crate::cards::combo_cards(idx);
        let (ra, rb) = (rank(a), rank(b));
        let hi = ra.max(rb);
        let lo = ra.min(rb);
        let name = crate::cards::hand_class(idx);
        let _ = (hi, lo);
        let e = map.entry(name).or_insert((0.0, 0));
        e.0 = e.0.max(*w);
        e.1 += 1;
    }
    let mut v: Vec<_> = map
        .into_iter()
        .map(|(k, (w, n))| (k, w, n))
        .collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    v.truncate(limit);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_and_plus() {
        let r = Range::from_str("QQ+").unwrap();
        assert_eq!(r.combos(), 18); // QQ, KK, AA
        let r = Range::from_str("AA").unwrap();
        assert_eq!(r.combos(), 6);
    }

    #[test]
    fn suited_offsuit() {
        assert_eq!(Range::from_str("AKs").unwrap().combos(), 4);
        assert_eq!(Range::from_str("AKo").unwrap().combos(), 12);
        assert_eq!(Range::from_str("AK").unwrap().combos(), 16);
        assert_eq!(Range::from_str("A2s+").unwrap().combos(), 12 * 4);
    }

    #[test]
    fn dash_ranges() {
        assert_eq!(Range::from_str("TT-77").unwrap().combos(), 24);
        assert_eq!(Range::from_str("A5s-A2s").unwrap().combos(), 16);
        assert_eq!(Range::from_str("T9s-76s").unwrap().combos(), 16);
    }

    #[test]
    fn weights_normalize() {
        let r = Range::from_str("AA:0.5,KK").unwrap();
        assert!((r.total() - 9.0).abs() < 1e-6);
        let n = r.normalized();
        assert!((n.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn invalid_input_rejected() {
        assert!(Range::from_str("ZZ").is_err());
        assert!(Range::from_str("AKs:2").is_err());
        assert!(Range::from_str(",,").is_err());
    }
}
