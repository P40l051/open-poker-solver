//! Human and machine readable solution reports.

use crate::cards::card_str;
use crate::cfr::Solution;
use crate::json::Json;
use crate::SolveConfig;

pub fn solution_json(sol: &Solution, cfg: &SolveConfig) -> Json {
    let nodes = sol
        .nodes
        .iter()
        .map(|n| {
            Json::obj(vec![
                ("id", Json::num(n.id as f64)),
                (
                    "parent",
                    match n.parent {
                        Some(p) => Json::num(p as f64),
                        None => Json::Null,
                    },
                ),
                ("street", Json::str(street_name(n.street))),
                ("board", Json::str(n.board.clone())),
                ("player", Json::str(if n.player == 0 { "OOP" } else { "IP" })),
                ("pot", Json::num(n.pot)),
                ("reach_oop", Json::num(n.reach_oop)),
                ("reach_ip", Json::num(n.reach_ip)),
                (
                    "edges",
                    Json::Arr(
                        n.edges
                            .iter()
                            .map(|e| {
                                Json::obj(vec![
                                    ("action_index", Json::num(e.action_index as f64)),
                                    ("action", Json::str(e.action.clone())),
                                    ("frequency", Json::num(e.frequency)),
                                    ("target", Json::num(e.target as f64)),
                                    (
                                        "chance",
                                        match &e.chance {
                                            Some(c) => Json::str(c.clone()),
                                            None => Json::Null,
                                        },
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "actions",
                    Json::Arr(
                        n.actions
                            .iter()
                            .map(|a| {
                                Json::obj(vec![
                                    ("label", Json::str(a.label.clone())),
                                    ("frequency", Json::num(a.frequency)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();

    Json::obj(vec![
        ("config", cfg.to_json()),
        (
            "summary",
            Json::obj(vec![
                ("iterations", Json::num(sol.iterations as f64)),
                ("ev_oop", Json::num(sol.ev_oop)),
                ("ev_ip", Json::num(sol.ev_ip)),
                ("br_oop", Json::num(sol.br_oop)),
                ("br_ip", Json::num(sol.br_ip)),
                ("exploitability", Json::num(sol.exploitability)),
                (
                    "exploitability_pct_pot",
                    Json::num(sol.exploitability_pct_pot),
                ),
                ("pot", Json::num(sol.pot)),
                ("terminals", Json::num(sol.terminals as f64)),
                ("decisions", Json::num(sol.decisions as f64)),
                ("chance_branches", Json::num(sol.chance_branches as f64)),
                ("elapsed_ms", Json::num(sol.elapsed_ms as f64)),
                (
                    "board",
                    Json::str(
                        sol.board
                            .iter()
                            .map(|c| card_str(*c))
                            .collect::<Vec<_>>()
                            .join(""),
                    ),
                ),
                (
                    "exploitability_history",
                    Json::Arr(
                        sol.exploitability_history
                            .iter()
                            .map(|(i, e)| {
                                Json::obj(vec![
                                    ("iteration", Json::num(*i as f64)),
                                    ("exploitability", Json::num(*e)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
        ("nodes", Json::Arr(nodes)),
    ])
}

pub fn text_summary(sol: &Solution, cfg: &SolveConfig) -> String {
    let mut s = String::new();
    let board = sol
        .board
        .iter()
        .map(|c| card_str(*c))
        .collect::<Vec<_>>()
        .join(" ");
    s.push_str(&format!("board        {board}\n"));
    s.push_str(&format!("pot / stack  {:.2} / {:.2}\n", cfg.pot, cfg.stack));
    s.push_str(&format!(
        "tree         {} decisions, {} terminals, {} chance branches\n",
        sol.decisions, sol.terminals, sol.chance_branches
    ));
    s.push_str(&format!(
        "solved       {} iterations in {} ms\n",
        sol.iterations, sol.elapsed_ms
    ));
    s.push_str(&format!(
        "ev           OOP {:+.4}   IP {:+.4}\n",
        sol.ev_oop, sol.ev_ip
    ));
    s.push_str(&format!(
        "best resp.   OOP {:+.4}   IP {:+.4}\n",
        sol.br_oop, sol.br_ip
    ));
    s.push_str(&format!(
        "exploit.     {:.5} chips ({:.2}% pot)\n",
        sol.exploitability, sol.exploitability_pct_pot
    ));
    s.push_str("\nroot strategy\n");
    for n in sol.nodes.iter().filter(|n| n.parent.is_none()) {
        s.push_str(&format!(
            "  {} on {} (pot {:.2})\n",
            if n.player == 0 { "OOP" } else { "IP" },
            n.board,
            n.pot
        ));
        for a in &n.actions {
            s.push_str(&format!("    {:<16} {:6.2}%\n", a.label, a.frequency * 100.0));
        }
    }
    s
}

fn street_name(s: u8) -> &'static str {
    match s {
        0 => "flop",
        1 => "turn",
        _ => "river",
    }
}
