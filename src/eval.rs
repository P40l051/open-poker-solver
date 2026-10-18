//! Seven-card hand evaluator.
//!
//! Produces a single `u32` where larger is stronger. The encoding is
//! `category << 24 | k1 << 16 | k2 << 12 | k3 << 8 | k4 << 4 | k5`
//! with kickers in descending order (`k1` is the most significant).

pub const HIGH_CARD: u32 = 0;
pub const PAIR: u32 = 1;
pub const TWO_PAIR: u32 = 2;
pub const TRIPS: u32 = 3;
pub const STRAIGHT: u32 = 4;
pub const FLUSH: u32 = 5;
pub const FULL_HOUSE: u32 = 6;
pub const QUADS: u32 = 7;
pub const STRAIGHT_FLUSH: u32 = 8;

#[inline]
fn pack(cat: u32, kickers: &[u32]) -> u32 {
    let mut v = cat << 24;
    let shifts = [16u32, 12, 8, 4, 0];
    for (i, k) in kickers.iter().take(5).enumerate() {
        v |= (*k & 0xF) << shifts[i];
    }
    v
}

/// Highest straight top-card for a 13-bit rank mask, or 0.
#[inline]
fn straight_high(mask: u16) -> Option<u32> {
    // Ace low straight: A,2,3,4,5 -> high card is the five.
    let a_low = mask & 0b1_0000_0000_0000 != 0;
    if a_low {
        let wheel = 0b0_0000_0000_1111; // 2,3,4,5
        if mask & wheel == wheel {
            return Some(5);
        }
    }
    for high in (6..=14).rev() {
        let start = high - 4; // lowest rank in the straight
        let mut need = 0u16;
        for r in start..=high {
            need |= 1 << (r - 2);
        }
        if mask & need == need {
            return Some(high);
        }
    }
    None
}

/// Evaluates any 5 to 7 cards.
pub fn eval(cards: &[u8]) -> u32 {
    debug_assert!((5..=7).contains(&cards.len()));
    let mut rank_count = [0u8; 15];
    let mut suit_mask = [0u16; 4];
    let mut suit_count = [0u8; 4];
    let mut rank_mask = 0u16;

    for &c in cards {
        let r = (c / 4 + 2) as usize;
        let s = (c % 4) as usize;
        rank_count[r] += 1;
        suit_mask[s] |= 1 << (r - 2);
        suit_count[s] += 1;
        rank_mask |= 1 << (r - 2);
    }

    // Straight flush first.
    for s in 0..4 {
        if suit_count[s] >= 5 {
            if let Some(high) = straight_high(suit_mask[s]) {
                return pack(STRAIGHT_FLUSH, &[high]);
            }
        }
    }

    // Rank groups, descending by multiplicity then rank.
    let mut quads = Vec::new();
    let mut trips = Vec::new();
    let mut pairs = Vec::new();
    let mut singles = Vec::new();
    for r in (2..=14).rev() {
        match rank_count[r] {
            4 => quads.push(r as u32),
            3 => trips.push(r as u32),
            2 => pairs.push(r as u32),
            1 => singles.push(r as u32),
            _ => {}
        }
    }

    if let Some(&q) = quads.first() {
        let kicker = singles.first().copied().unwrap_or_else(|| {
            pairs
                .first()
                .copied()
                .or_else(|| trips.first().copied())
                .unwrap_or(0)
        });
        return pack(QUADS, &[q, kicker]);
    }

    if let (Some(&t), Some(&p)) = (trips.first(), pairs.first()) {
        return pack(FULL_HOUSE, &[t, p]);
    }
    if trips.len() >= 2 {
        return pack(FULL_HOUSE, &[trips[0], trips[1]]);
    }

    for s in 0..4 {
        if suit_count[s] >= 5 {
            let mut ks = Vec::with_capacity(5);
            for r in (2..=14).rev() {
                if suit_mask[s] & (1 << (r - 2)) != 0 {
                    ks.push(r as u32);
                    if ks.len() == 5 {
                        break;
                    }
                }
            }
            return pack(FLUSH, &ks);
        }
    }

    if let Some(high) = straight_high(rank_mask) {
        return pack(STRAIGHT, &[high]);
    }

    if let Some(&t) = trips.first() {
        let ks: Vec<u32> = std::iter::once(t)
            .chain(singles.iter().copied())
            .chain(pairs.iter().copied())
            .take(3)
            .collect();
        return pack(TRIPS, &ks);
    }

    if pairs.len() >= 2 {
        let (p1, p2) = (pairs[0], pairs[1]);
        let kicker = singles.first().copied().unwrap_or(0);
        return pack(TWO_PAIR, &[p1, p2, kicker]);
    }

    if let Some(&p) = pairs.first() {
        let ks: Vec<u32> = std::iter::once(p).chain(singles.iter().copied()).take(4).collect();
        return pack(PAIR, &ks);
    }

    let ks: Vec<u32> = singles.iter().copied().take(5).collect();
    pack(HIGH_CARD, &ks)
}

/// Convenience wrapper for 2 hole cards plus 5 board cards.
#[inline]
pub fn eval7(hole: (u8, u8), board: &[u8]) -> u32 {
    debug_assert_eq!(board.len(), 5);
    let cards = [hole.0, hole.1, board[0], board[1], board[2], board[3], board[4]];
    eval(&cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::parse_card;

    fn hand(s: &str) -> u32 {
        let cards: Vec<u8> = s.split_whitespace().map(|t| parse_card(t).unwrap()).collect();
        eval(&cards)
    }

    #[test]
    fn category_ordering() {
        let sf = hand("As Ks Qs Js Ts 2d 3c");
        let quads = hand("Ah Ad Ac As Kd 2c 3c");
        let boat = hand("Kh Kd Kc 2s 2d 3c 4c");
        let flush = hand("Ah Jh 8h 5h 2h Kd Qc");
        let straight = hand("9h 8d 7c 6s 5h Ad Kc");
        let trips = hand("Qh Qd Qc 9s 4d 2c 3c");
        let two = hand("Jh Jd 8c 8s Ad 3c 2h");
        let pair = hand("Th Td 9c 5s 3d 2c 4h");
        let high = hand("Ah Kd 9c 7s 4d 3c 2h");
        let order = [high, pair, two, trips, straight, flush, boat, quads, sf];
        for w in order.windows(2) {
            assert!(w[1] > w[0], "{:#x} !> {:#x}", w[1], w[0]);
        }
    }

    #[test]
    fn wheel_is_a_straight() {
        let wheel = hand("Ah 2d 3c 4s 5h Kd Qc");
        let six_high = hand("2h 3d 4c 5s 6h Kd Qc");
        assert_eq!(wheel >> 24, STRAIGHT);
        assert!(six_high > wheel);
    }

    #[test]
    fn kickers_matter() {
        let a = hand("Ah Ad Kc Qs 9h 3c 2d");
        let b = hand("Ah Ad Kc Js 9h 3c 2d");
        assert!(a > b);
    }

    #[test]
    fn board_plays() {
        let board = "As Ks Qs Js Ts".split_whitespace().map(|x| parse_card(x).unwrap()).collect::<Vec<_>>();
        let a = eval7((parse_card("2c").unwrap(), parse_card("3d").unwrap()), &board);
        let b = eval7((parse_card("9h").unwrap(), parse_card("9d").unwrap()), &board);
        assert_eq!(a, b, "board royal flush plays for both");
        assert_eq!(a >> 24, STRAIGHT_FLUSH);
    }
}
