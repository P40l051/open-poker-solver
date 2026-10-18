//! Card and combo primitives.
//!
//! A card is `0..52`, encoded as `rank * 4 + suit` where rank `0 = deuce`
//! and `12 = ace`. A two-card combo is indexed into the 1326-element space
//! used by every range vector in the solver.

/// Number of two-card starting combos.
pub const NCOMBOS: usize = 1326;

const RANK_CHARS: [u8; 13] = *b"23456789TJQKA";
const SUIT_CHARS: [u8; 4] = *b"cdhs";

#[inline]
pub fn card(rank: u8, suit: u8) -> u8 {
    debug_assert!(rank < 13 && suit < 4);
    rank * 4 + suit
}

#[inline]
pub fn rank(c: u8) -> u8 {
    c / 4 + 2
}

#[inline]
pub fn suit(c: u8) -> u8 {
    c % 4
}

/// Parses `As`, `Td`, `2c`. Case insensitive on the rank, suit must be lowercase or uppercase.
pub fn parse_card(s: &str) -> Option<u8> {
    let b = s.as_bytes();
    if b.len() != 2 {
        return None;
    }
    let r = RANK_CHARS
        .iter()
        .position(|&x| x == b[0].to_ascii_uppercase())? as u8;
    let su = SUIT_CHARS
        .iter()
        .position(|&x| x == b[1].to_ascii_lowercase())? as u8;
    Some(card(r, su))
}

pub fn card_str(c: u8) -> String {
    let mut s = String::with_capacity(2);
    s.push(RANK_CHARS[(c / 4) as usize] as char);
    s.push(SUIT_CHARS[(c % 4) as usize] as char);
    s
}

/// Parses a board such as `AhKd7c` or `Ah Kd 7c`.
pub fn parse_board(s: &str) -> Result<Vec<u8>, String> {
    let compact: String = s.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    if compact.len() % 2 != 0 {
        return Err(format!("board has odd length: {s:?}"));
    }
    let mut out = Vec::new();
    for chunk in compact.as_bytes().chunks(2) {
        let part = std::str::from_utf8(chunk).unwrap();
        let c = parse_card(part).ok_or_else(|| format!("invalid card {part:?}"))?;
        if out.contains(&c) {
            return Err(format!("duplicate card {part:?}"));
        }
        out.push(c);
    }
    Ok(out)
}

/// Dense combo index for `a < b`.
#[inline]
pub fn combo_index(a: u8, b: u8) -> usize {
    debug_assert!(a < b);
    let (a, b) = (a as usize, b as usize);
    51 * a - a * (a.saturating_sub(1)) / 2 + (b - a - 1)
}

/// Inverse of [`combo_index`].
#[inline]
pub fn combo_cards(idx: usize) -> (u8, u8) {
    debug_assert!(idx < NCOMBOS);
    // Find the first card: cumulative count for card a is 51+50+...+(51-a).
    let mut a = 0usize;
    let mut remaining = idx;
    loop {
        let count = 51 - a;
        if remaining < count {
            return (a as u8, (a + 1 + remaining) as u8);
        }
        remaining -= count;
        a += 1;
    }
}

pub fn combo_str(idx: usize) -> String {
    let (a, b) = combo_cards(idx);
    format!("{}{}", card_str(a), card_str(b))
}

/// Bitmask (bit per card) for a combo.
#[inline]
pub fn combo_mask(idx: usize) -> u64 {
    let (a, b) = combo_cards(idx);
    (1u64 << a) | (1u64 << b)
}

/// True when the combo shares no card with `mask`.
#[inline]
pub fn combo_playable(idx: usize, mask: u64) -> bool {
    combo_mask(idx) & mask == 0
}

/// `true` for a pocket pair combo.
#[inline]
pub fn is_pair(idx: usize) -> bool {
    let (a, b) = combo_cards(idx);
    rank(a) == rank(b)
}

/// Canonical hand class such as `AKs`, `QQ`, used for compact range output.
pub fn hand_class(idx: usize) -> String {
    let (a, b) = combo_cards(idx);
    let (ra, rb) = (rank(a), rank(b));
    let hi = RANK_CHARS[(ra.max(rb) - 2) as usize] as char;
    let lo = RANK_CHARS[(ra.min(rb) - 2) as usize] as char;
    if ra == rb {
        format!("{hi}{lo}")
    } else if suit(a) == suit(b) {
        format!("{hi}{lo}s")
    } else {
        format!("{hi}{lo}o")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_index_is_bijective() {
        let mut seen = std::collections::HashSet::new();
        for idx in 0..NCOMBOS {
            let (a, b) = combo_cards(idx);
            assert!(a < b, "{idx} -> {a},{b}");
            assert!(seen.insert((a, b)));
            assert_eq!(combo_index(a, b), idx);
        }
        assert_eq!(seen.len(), NCOMBOS);
    }

    #[test]
    fn card_roundtrip() {
        for c in 0..52 {
            assert_eq!(parse_card(&card_str(c)), Some(c));
        }
        assert_eq!(card_str(card(12, 0)), "Ac");
        assert_eq!(parse_board("AhKd7c").unwrap().len(), 3);
        assert!(parse_board("AhAh").is_err());
    }
}
