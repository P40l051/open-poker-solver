//! `ops` — command line interface for the postflop solver.

use open_poker_solver::hh;
use open_poker_solver::json::Json;
use open_poker_solver::report::{solution_json, text_summary};
use open_poker_solver::{solve, SolveConfig};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn usage() -> String {
    r#"ops — vector CFR+ postflop solver

USAGE
  ops [options]

OPTIONS
  --hand-file <path>     PokerStars hand history: derive board/pot/stack/sizes
  --board <cards>        flop/turn/river, e.g. AhKd7c or AhKd7c2s
  --oop <range>          out-of-position range, e.g. "QQ+,AKs,AQo"
  --ip <range>           in-position range
  --pot <chips>          pot at the start of the solve (default 10)
  --stack <chips>        effective stack behind (default 90)
  --sizes <f1,f2>        bet sizes as pot fractions for every street
  --turn-sizes <f1,f2>   override sizes for the turn
  --river-sizes <f1,f2>  override sizes for the river
  --raises <n>           max raises per street (default 1)
  --rake-pct <n>         rake percentage of the pot, e.g. 5 (default 0)
  --rake-cap <chips>     rake cap, 0 = uncapped (default 0)
  --runouts <n>          sample n rivers per turn (0 = all; flop defaults to 4)
  --turn-samples <n>     sample n turn cards on a flop (default 7, 0 = all)
  --iterations <n>       CFR iterations (default 400)
  --target-expl <x>      early stop at this exploitability in chips
  --check-every <n>      report/exploitability period (default 25)
  --max-nodes <n>        tree safety cap (default 400000)
  --json                 print JSON instead of text
  --out <path>           write output to a file
  --quiet                no progress on stderr
  -h, --help

Ranges accept: AA, TT+, A2s+, T9s-76s, AKo, random, plus ":weight".
"#
    .to_string()
}

fn parse_sizes(s: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let t = part.trim();
        if t.is_empty() {
            continue;
        }
        out.push(t.parse::<f64>().map_err(|_| format!("bad size {t:?}"))?);
    }
    if out.is_empty() {
        return Err("empty size list".into());
    }
    Ok(out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg = SolveConfig::default();
    let mut json_out = false;
    let mut out_path: Option<String> = None;
    let mut quiet = false;
    let mut turn_sizes: Option<Vec<f64>> = None;
    let mut river_sizes: Option<Vec<f64>> = None;
    let mut hand_file: Option<String> = None;
    let mut ranges_given = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let next = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("missing value for {a}"))
        };
        let r: Result<(), String> = (|| {
            match a {
                "-h" | "--help" => {
                    print!("{}", usage());
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("ops {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "--board" => cfg.board = next(&mut i)?,
                "--oop" => {
                    cfg.oop_range = next(&mut i)?;
                    ranges_given = true;
                }
                "--ip" => {
                    cfg.ip_range = next(&mut i)?;
                    ranges_given = true;
                }
                "--hand-file" => hand_file = Some(next(&mut i)?),
                "--pot" => cfg.pot = next(&mut i)?.parse().map_err(|_| "bad pot")?,
                "--stack" => cfg.stack = next(&mut i)?.parse().map_err(|_| "bad stack")?,
                "--sizes" => cfg.bet_sizes = vec![parse_sizes(&next(&mut i)?)?; 3],
                "--turn-sizes" => turn_sizes = Some(parse_sizes(&next(&mut i)?)?),
                "--river-sizes" => river_sizes = Some(parse_sizes(&next(&mut i)?)?),
                "--raises" => cfg.max_raises = next(&mut i)?.parse().map_err(|_| "bad raises")?,
                "--rake-pct" => {
                    let pct: f64 = next(&mut i)?.parse().map_err(|_| "bad rake-pct")?;
                    cfg.rake_pct = pct / 100.0;
                }
                "--rake-cap" => {
                    cfg.rake_cap = next(&mut i)?.parse().map_err(|_| "bad rake-cap")?
                }
                "--runouts" => {
                    cfg.runout_samples = next(&mut i)?.parse().map_err(|_| "bad runouts")?
                }
                "--turn-samples" => {
                    cfg.turn_samples = next(&mut i)?.parse().map_err(|_| "bad turn-samples")?
                }
                "--iterations" => {
                    cfg.iterations = next(&mut i)?.parse().map_err(|_| "bad iterations")?
                }
                "--target-expl" => {
                    cfg.target_exploitability = next(&mut i)?.parse().map_err(|_| "bad target")?
                }
                "--check-every" => {
                    cfg.check_every = next(&mut i)?.parse().map_err(|_| "bad check-every")?
                }
                "--max-nodes" => {
                    cfg.max_nodes = next(&mut i)?.parse().map_err(|_| "bad max-nodes")?
                }
                "--json" => json_out = true,
                "--quiet" => quiet = true,
                "--out" => out_path = Some(next(&mut i)?),
                other => return Err(format!("unknown option {other}")),
            }
            Ok(())
        })();
        if let Err(e) = r {
            eprintln!("error: {e}\n\n{}", usage());
            std::process::exit(2);
        }
        i += 1;
    }

    // Hand-history import fills the spot; ranges still come from the user.
    let mut imported_spot = None;
    if let Some(path) = &hand_file {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {path}: {e}"))
            .unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });
        let hand = hh::parse(&text).unwrap_or_else(|e| {
            eprintln!("hand parse error: {e}");
            std::process::exit(1);
        });
        let spot = hand.spot().unwrap_or_else(|e| {
            eprintln!("cannot derive a spot: {e}");
            std::process::exit(1);
        });
        if !quiet {
            eprintln!("{}", hh::spot_summary(&spot));
        }
        cfg.board = spot
            .board
            .iter()
            .map(|c| open_poker_solver::cards::card_str(*c))
            .collect();
        cfg.pot = spot.pot_bb;
        cfg.stack = spot.stack_bb;
        cfg.bet_sizes = vec![spot.bet_sizes.clone(); 3];
        imported_spot = Some(spot);
    }
    if hand_file.is_some() && !ranges_given {
        eprintln!("spot imported. Pass --oop and --ip to solve it.");
        std::process::exit(0);
    }
    if let Some(spot) = &imported_spot {
        if !quiet {
            for n in &spot.notes {
                eprintln!("note: {n}");
            }
            for w in &spot.warnings {
                eprintln!("warning: {w}");
            }
        }
    }

    // Apply per-street overrides relative to the solved streets.
    let board_len = cfg.board.chars().filter(|c| !c.is_whitespace()).count() / 2;
    if turn_sizes.is_some() || river_sizes.is_some() {
        let rounds = 5usize.saturating_sub(board_len) + 1;
        let base = cfg.bet_sizes[0].clone();
        let mut sizes = Vec::new();
        for i in 0..rounds {
            let street = board_len as i64 - 3 + i as i64; // 0 flop, 1 turn, 2 river
            let list = match street {
                1 => turn_sizes.clone().unwrap_or_else(|| base.clone()),
                2 => river_sizes
                    .clone()
                    .unwrap_or_else(|| turn_sizes.clone().unwrap_or_else(|| base.clone())),
                _ => base.clone(),
            };
            sizes.push(list);
        }
        cfg.bet_sizes = sizes;
    }

    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    let mut last_print = Instant::now();
    let result = solve(&cfg, &cancel, |p| {
        if !quiet && last_print.elapsed().as_millis() > 200 {
            eprintln!(
                "  iter {:>5}/{}  expl {:.5}  ev_oop {:+.4}",
                p.iteration, p.iterations, p.exploitability, p.ev_oop
            );
            last_print = Instant::now();
        }
    });

    match result {
        Ok((sol, _solver)) => {
            let text = if json_out {
                solution_json(&sol, &cfg).to_string()
            } else {
                text_summary(&sol, &cfg)
            };
            if let Some(path) = out_path {
                if let Err(e) = std::fs::write(&path, &text) {
                    eprintln!("error writing {path}: {e}");
                    std::process::exit(1);
                }
                if !quiet {
                    eprintln!("written {path}");
                }
            } else {
                println!("{text}");
            }
            if !quiet {
                eprintln!("done in {:.1}s", start.elapsed().as_secs_f64());
            }
        }
        Err(e) => {
            eprintln!("solve failed: {e}");
            std::process::exit(1);
        }
    }
    let _ = Json::Null;
}
