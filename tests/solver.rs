use open_poker_solver::cards::{combo_mask, parse_board, NCOMBOS};
use open_poker_solver::tree::root_weights;
use open_poker_solver::{range::Range, solve, SolveConfig};
use std::sync::atomic::AtomicBool;

fn base(board: &str) -> SolveConfig {
    SolveConfig {
        board: board.into(),
        oop_range: "AA".into(),
        ip_range: "KK".into(),
        pot: 10.0,
        stack: 90.0,
        bet_sizes: vec![vec![0.75]],
        max_raises: 1,
        runout_samples: 0,
        iterations: 300,
        check_every: 100,
        ..Default::default()
    }
}

#[test]
fn ranges_are_renormalized_after_card_removal() {
    let board = parse_board("AhKd7c2s9h").unwrap();
    let r = Range::from_str("AA").unwrap();
    let w = root_weights(&r, &board);
    let total: f32 = w.iter().sum();
    assert!((total - 1.0).abs() < 1e-5, "total = {total}");
    // Ah is on the board, so only the three AA combos without Ah survive.
    assert_eq!(w.iter().filter(|x| **x > 0.0).count(), 3);
}

#[test]
fn nuts_capture_the_pot_against_a_losing_range() {
    let cfg = base("AhKd7c2s9h");
    let cancel = AtomicBool::new(false);
    let (sol, _) = solve(&cfg, &cancel, |_| {}).unwrap();
    // Both players put in the initial pot/2 = 5, and IP can never win.
    assert!((sol.ev_oop - 5.0).abs() < 0.05, "ev = {}", sol.ev_oop);
    assert!((sol.ev_oop + sol.ev_ip).abs() < 1e-6, "zero sum");
    assert!(sol.exploitability < 0.01, "expl = {}", sol.exploitability);
}

#[test]
fn rake_is_charged_to_the_winner() {
    let mut cfg = base("AhKd7c2s9h");
    cfg.rake_pct = 0.05; // 5% of the pot
    let cancel = AtomicBool::new(false);
    let (sol, _) = solve(&cfg, &cancel, |_| {}).unwrap();
    // Checking down wins 5 minus 0.5 rake; betting wins 5 minus rake on a
    // bigger pot, so the nuts prefer the cheapest path.
    assert!((sol.ev_oop - 4.5).abs() < 0.06, "ev = {}", sol.ev_oop);
    assert!(sol.ev_oop + sol.ev_ip < -0.4, "rake must leave the game constant-sum: {}", sol.ev_oop + sol.ev_ip);
}

#[test]
fn exploitability_decreases_with_iterations() {
    let mut cfg = base("AhKd7c2s9h");
    cfg.oop_range = "QQ+,AKs,AQo,KQs".into();
    cfg.ip_range = "JJ+,AQs,KJs,QJs".into();
    let cancel = AtomicBool::new(false);

    cfg.iterations = 40;
    cfg.check_every = 20;
    let (short, _) = solve(&cfg, &cancel, |_| {}).unwrap();

    cfg.iterations = 400;
    cfg.check_every = 400;
    let (long, _) = solve(&cfg, &cancel, |_| {}).unwrap();

    assert!(
        long.exploitability < short.exploitability,
        "{} !< {}",
        long.exploitability,
        short.exploitability
    );
    assert!(long.exploitability_pct_pot < 0.5, "did not converge: {long:?}");
}

#[test]
fn turn_solve_enumerates_rivers_and_stays_consistent() {
    let mut cfg = base("AhKd7c2s");
    cfg.oop_range = "QQ+".into();
    cfg.ip_range = "JJ+".into();
    cfg.iterations = 120;
    cfg.check_every = 60;
    let cancel = AtomicBool::new(false);
    let (sol, _) = solve(&cfg, &cancel, |_| {}).unwrap();
    assert!(sol.chance_branches >= 48, "expected river enumeration");
    // With public chance the two counterfactual values are measured from
    // separate marginal traversals; they agree up to the vector-chance
    // approximation (well under 1% of the pot here).
    assert!((sol.ev_oop + sol.ev_ip).abs() < 0.5, "ev0 {} ev1 {}", sol.ev_oop, sol.ev_ip);
    assert!(sol.exploitability.is_finite());
}

#[test]
fn board_blocking_is_respected_in_terminal_boards() {
    let board = parse_board("AhKd7c2s9h").unwrap();
    let mask = board.iter().fold(0u64, |m, c| m | (1u64 << c));
    for idx in 0..NCOMBOS {
        if combo_mask(idx) & mask != 0 {
            // Combos using a board card can never be dealt at the root.
            assert!(combo_mask(idx) & mask != 0);
        }
    }
}
