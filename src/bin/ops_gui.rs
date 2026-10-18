//! `ops-gui` — local desktop app: HTTP API + embedded single-page UI.
//!
//! There are no external GUI crates available in this offline toolchain, so the
//! desktop experience is a local Rust server plus a chromeless browser window
//! (`--app=`) pointed at 127.0.0.1. Everything runs on this machine; nothing is
//! sent anywhere.

use open_poker_solver::cfr::{Solution, Solver};
use open_poker_solver::hh;
use open_poker_solver::json::Json;
use open_poker_solver::presets;
use open_poker_solver::report::solution_json;
use open_poker_solver::{cards::card_str, solve, SolveConfig};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const INDEX_HTML: &str = include_str!("../../assets/index.html");
const TUTORIAL_MD: &str = include_str!("../../TUTORIAL.md");
const ICON_SVG: &str = include_str!("../../assets/icon.svg");

struct JobState {
    id: u64,
    status: &'static str,
    iteration: usize,
    iterations: usize,
    exploitability: f64,
    ev_oop: f64,
    message: Option<String>,
    solution: Option<Solution>,
    solver: Option<Arc<Mutex<Solver>>>,
    cfg: SolveConfig,
    started: Instant,
}

type Job = Arc<Mutex<JobState>>;

#[derive(Default)]
struct Store {
    jobs: Vec<(u64, Job)>,
    next_id: u64,
    shutdown_enabled: bool,
}

impl Store {
    fn create(&mut self, cfg: SolveConfig, cancel: Arc<AtomicBool>) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        CANCELS.lock().unwrap().push((id, Arc::clone(&cancel)));
        let cfg_for_job = cfg.clone();
        let job: Job = Arc::new(Mutex::new(JobState {
            id,
            status: "running",
            iteration: 0,
            iterations: cfg.iterations,
            exploitability: 0.0,
            ev_oop: 0.0,
            message: None,
            solution: None,
            solver: None,
            cfg: cfg_for_job,
            started: Instant::now(),
        }));
        self.jobs.push((id, Arc::clone(&job)));

        std::thread::spawn(move || {
            let hb = Arc::clone(&job);
            let result = solve(&cfg, &cancel, move |p| {
                if let Ok(mut j) = hb.lock() {
                    j.iteration = p.iteration;
                    j.iterations = p.iterations;
                    j.exploitability = p.exploitability;
                    j.ev_oop = p.ev_oop;
                }
            });
            let mut j = job.lock().unwrap();
            match result {
                Ok((solution, solver)) => {
                    j.status = if cancel.load(Ordering::Relaxed) {
                        "cancelled"
                    } else {
                        "done"
                    };
                    j.solution = Some(solution);
                    j.solver = Some(Arc::new(Mutex::new(solver)));
                }
                Err(e) => {
                    j.status = if e == "cancelled" { "cancelled" } else { "error" };
                    j.message = Some(e);
                }
            }
        });
        id
    }

    fn get(&self, id: u64) -> Option<Job> {
        self.jobs
            .iter()
            .find(|(jid, _)| *jid == id)
            .map(|(_, j)| Arc::clone(j))
    }
}

fn main() {
    let mut port: u16 = 7788;
    let mut open = true;
    let mut shutdown_api = true;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args.get(i).and_then(|x| x.parse().ok()).unwrap_or(port);
            }
            "--no-open" => open = false,
            "--no-shutdown-api" => shutdown_api = false,
            "--version" | "-V" => {
                println!("ops-gui {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "-h" | "--help" => {
                println!("ops-gui [--port 7788] [--no-open] [--no-shutdown-api]");
                println!("  --version   print the version");
                return;
            }
            other => {
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    // Pick a free port: another instance may already own the default one.
    let mut listener = None;
    for candidate in port..port.saturating_add(20) {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", candidate)) {
            port = candidate;
            listener = Some(l);
            break;
        }
    }
    let listener = match listener {
        Some(l) => l,
        None => {
            eprintln!("cannot bind any port in {port}..{}", port + 20);
            std::process::exit(1);
        }
    };
    let url = format!("http://127.0.0.1:{port}/");
    println!("OPS — Open Poker Solver");
    println!("listening on {url}");
    println!("API: POST /api/solve · GET /api/job?id=N · GET /api/node?id=N&node=M");

    if open {
        open_window(&url);
    }

    let store = Arc::new(Mutex::new(Store {
        shutdown_enabled: shutdown_api,
        ..Store::default()
    }));
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    let _ = handle(s, store);
                });
            }
            Err(e) => eprintln!("connection error: {e}"),
        }
    }
}

fn open_window(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        if std::path::Path::new(chrome).exists() {
            let _ = std::process::Command::new(chrome)
                .arg(format!("--app={url}"))
                .spawn();
        } else {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        eprintln!("open {url}");
    }
}

fn handle(mut stream: TcpStream, store: Arc<Mutex<Store>>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    let q = |key: &str| -> Option<String> {
        query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            if k == key {
                Some(v.to_string())
            } else {
                None
            }
        })
    };

    let (status, ctype, payload) = match (method.as_str(), path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string()),
        ("GET", "/tutorial") | ("GET", "/tutorial/") => (
            "200 OK",
            "text/html; charset=utf-8",
            tutorial_page(),
        ),
        ("GET", "/tutorial.md") => ("200 OK", "text/plain; charset=utf-8", TUTORIAL_MD.to_string()),
        ("GET", "/icon.svg") => ("200 OK", "image/svg+xml", ICON_SVG.to_string()),
        ("POST", "/api/shutdown") => {
            if !store.lock().unwrap().shutdown_enabled {
                ("403 Forbidden", "application/json", err_json("shutdown disabled"))
            } else {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    std::process::exit(0);
                });
                ("200 OK", "application/json", Json::obj(vec![("ok", Json::Bool(true))]).to_string())
            }
        }
        ("GET", "/api/health") => (
            "200 OK",
            "application/json",
            Json::obj(vec![("ok", Json::Bool(true))]).to_string(),
        ),
        ("POST", "/api/import") => match Json::parse(&body) {
            Ok(v) => {
                let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
                match hh::parse(text).and_then(|h| h.spot()) {
                    Ok(spot) => {
                        let board: String = spot.board.iter().map(|c| card_str(*c)).collect();
                        let cards: Vec<Json> = spot
                            .hero_cards
                            .iter()
                            .map(|c| Json::str(card_str(*c)))
                            .collect();
                        let js = Json::obj(vec![
                            ("street", Json::str(spot.street.clone())),
                            ("board", Json::str(board)),
                            ("pot_bb", Json::num(spot.pot_bb)),
                            ("stack_bb", Json::num(spot.stack_bb)),
                            ("pot_chips", Json::num(spot.pot_chips)),
                            ("stack_chips", Json::num(spot.stack_chips)),
                            ("sb", Json::num(spot.sb)),
                            ("bb", Json::num(spot.bb)),
                            ("oop", Json::str(spot.oop.clone())),
                            ("ip", Json::str(spot.ip.clone())),
                            ("oop_position", Json::str(spot.oop_position.clone())),
                            ("ip_position", Json::str(spot.ip_position.clone())),
                            ("preflop", preflop_json(&spot.preflop)),
                            (
                                "suggested_ranges",
                                match presets::suggest_ranges(&spot) {
                                    Some(s) => suggestion_json(&s),
                                    None => Json::Null,
                                },
                            ),
                            ("hero", Json::str(spot.hero.clone().unwrap_or_default())),
                            ("hero_cards", Json::Arr(cards)),
                            ("rake", Json::num(spot.rake)),
                            ("total_pot", Json::num(spot.total_pot)),
                            (
                                "rake_pct_observed",
                                Json::num(spot.rake_pct_observed),
                            ),
                            (
                                "bet_sizes",
                                Json::Arr(spot.bet_sizes.iter().map(|x| Json::num(*x)).collect()),
                            ),
                            (
                                "notes",
                                Json::Arr(spot.notes.iter().cloned().map(Json::Str).collect()),
                            ),
                            (
                                "warnings",
                                Json::Arr(spot.warnings.iter().cloned().map(Json::Str).collect()),
                            ),
                            ("summary", Json::str(hh::spot_summary(&spot))),
                        ]);
                        ("200 OK", "application/json", js.to_string())
                    }
                    Err(e) => ("400 Bad Request", "application/json", err_json(&e)),
                }
            }
            Err(e) => ("400 Bad Request", "application/json", err_json(&e)),
        },
        ("POST", "/api/solve") => match Json::parse(&body).map(|v| SolveConfig::from_json(&v)) {
            Ok(Ok(cfg)) => {
                let cancel = Arc::new(AtomicBool::new(false));
                let mut st = store.lock().unwrap();
                let id = st.create(cfg, cancel);
                (
                    "200 OK",
                    "application/json",
                    Json::obj(vec![("id", Json::num(id as f64))]).to_string(),
                )
            }
            Ok(Err(e)) => ("400 Bad Request", "application/json", err_json(&e)),
            Err(e) => ("400 Bad Request", "application/json", err_json(&e)),
        },
        ("GET", "/api/job") => {
            let id = q("id").and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            let job = store.lock().unwrap().get(id);
            match job {
                Some(job) => {
                    let j = job.lock().unwrap();
                    (
                        "200 OK",
                        "application/json",
                        job_json(&j),
                    )
                }
                None => ("404 Not Found", "application/json", err_json("unknown job")),
            }
        }
        ("POST", "/api/cancel") => {
            // Cancellation is cooperative: the running thread checks the flag
            // every iteration. We do not keep the flag here, so expose it via
            // the job's solution path only after a run; a full cancel handle is
            // kept in a side table below.
            let id = q("id").and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            CANCELS.lock().unwrap().iter().find(|(jid, _)| *jid == id).map(|(_, c)| c.store(true, Ordering::Relaxed));
            ("200 OK", "application/json", Json::obj(vec![("ok", Json::Bool(true))]).to_string())
        }
        ("GET", "/api/node") => {
            let id = q("id").and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            let node = q("node").and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
            let job = store.lock().unwrap().get(id);
            match job {
                Some(job) => {
                    let j = job.lock().unwrap();
                    match (&j.solver, &j.solution) {
                        (Some(solver), Some(sol)) => {
                            let s = solver.lock().unwrap();
                            let hands = s.combo_strategy(node);
                            let actions: Vec<String> = sol
                                .nodes
                                .iter()
                                .find(|n| n.id == node)
                                .map(|n| n.actions.iter().map(|a| a.label.clone()).collect())
                                .unwrap_or_default();
                            let board = s
                                .tree
                                .nodes
                                .get(node)
                                .map(|n| n.board.iter().map(|c| card_str(*c)).collect::<String>())
                                .unwrap_or_default();
                            let js = Json::obj(vec![
                                ("node", Json::num(node as f64)),
                                ("board", Json::str(board)),
                                ("actions", Json::Arr(actions.into_iter().map(Json::Str).collect())),
                                (
                                    "hands",
                                    Json::Arr(
                                        hands
                                            .into_iter()
                                            .map(|(hand, weight, freq)| {
                                                Json::obj(vec![
                                                    ("hand", Json::str(hand)),
                                                    ("weight", Json::num(weight)),
                                                    (
                                                        "freq",
                                                        Json::Arr(
                                                            freq.into_iter().map(Json::num).collect(),
                                                        ),
                                                    ),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                ),
                            ]);
                            ("200 OK", "application/json", js.to_string())
                        }
                        _ => ("409 Conflict", "application/json", err_json("solve not finished")),
                    }
                }
                None => ("404 Not Found", "application/json", err_json("unknown job")),
            }
        }
        _ => ("404 Not Found", "application/json", err_json("not found")),
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.as_bytes().len()
    );
    stream.write_all(response.as_bytes())?;
    stream.write_all(payload.as_bytes())?;
    stream.flush()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn tutorial_page() -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>ops — guide</title>
<style>
 body{{margin:0;background:#0d1117;color:#e6edf3;font:13px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif}}
 header{{position:sticky;top:0;display:flex;gap:14px;align-items:center;padding:12px 20px;background:#161b22;border-bottom:1px solid #2a3444}}
 header a{{color:#58a6ff;text-decoration:none}}
 header .sp{{flex:1}}
 main{{max-width:980px;margin:0 auto;padding:24px}}
 pre{{white-space:pre-wrap;word-wrap:break-word;font:12px/1.55 ui-monospace,SFMono-Regular,Menlo,monospace;background:#161b22;border:1px solid #2a3444;border-radius:10px;padding:20px}}
 h1{{font-size:15px;margin:0;letter-spacing:.3px}}
</style></head>
<body>
<header>
  <h1>ops — guide</h1><span class="sp"></span>
  <a href="/">← app</a>
  <a href="/tutorial.md" target="_blank">markdown</a>
</header>
<main><pre>{}</pre></main>
</body></html>"#,
        html_escape(TUTORIAL_MD)
    )
}

fn err_json(msg: &str) -> String {
    Json::obj(vec![("error", Json::str(msg))]).to_string()
}

fn opt_num(v: Option<f64>) -> Json {
    match v {
        Some(x) => Json::num(x),
        None => Json::Null,
    }
}

fn preflop_player_json(p: &hh::PreflopPlayer) -> Json {
    Json::obj(vec![
        ("name", Json::str(p.name.clone())),
        ("position", Json::str(p.position.clone())),
        ("label", Json::str(p.label())),
    ])
}

fn preflop_json(pl: &hh::PreflopLine) -> Json {
    let optional_player = |p: &Option<hh::PreflopPlayer>| match p {
        Some(p) => preflop_player_json(p),
        None => Json::Null,
    };
    Json::obj(vec![
        ("pot_type", Json::str(pl.pot_type.short())),
        ("pot_type_label", Json::str(pl.pot_type.label())),
        ("raises", Json::num(pl.raises as f64)),
        ("summary", Json::str(pl.summary())),
        ("roles", Json::str(pl.roles())),
        ("opener", optional_player(&pl.opener)),
        ("last_aggressor", optional_player(&pl.last_aggressor)),
        ("open_to", opt_num(pl.open_to)),
        ("open_to_bb", opt_num(pl.open_to_bb)),
        (
            "callers",
            Json::Arr(pl.callers.iter().map(preflop_player_json).collect()),
        ),
    ])
}

fn suggestion_json(s: &presets::RangeSuggestion) -> Json {
    Json::obj(vec![
        ("oop_range", Json::str(s.oop_range.clone())),
        ("ip_range", Json::str(s.ip_range.clone())),
        ("source", Json::str(s.source.clone())),
        (
            "notes",
            Json::Arr(s.notes.iter().cloned().map(Json::Str).collect()),
        ),
    ])
}

fn job_json(j: &JobState) -> String {
    let result = j
        .solution
        .as_ref()
        .map(|s| solution_json(s, &j.cfg));
    let progress = Json::obj(vec![
        ("iteration", Json::num(j.iteration as f64)),
        ("iterations", Json::num(j.iterations as f64)),
        ("exploitability", Json::num(j.exploitability)),
        ("ev_oop", Json::num(j.ev_oop)),
        ("elapsed_ms", Json::num(j.started.elapsed().as_millis() as f64)),
    ]);
    let mut fields = vec![
        ("id", Json::num(j.id as f64)),
        ("status", Json::str(j.status)),
        ("progress", progress),
    ];
    if let Some(r) = result {
        fields.push(("result", r));
    }
    if let Some(m) = &j.message {
        fields.push(("message", Json::str(m.clone())));
    }
    Json::obj(fields).to_string()
}

/// Cancel handles keyed by job id (cooperative flag shared with the worker).
static CANCELS: Mutex<Vec<(u64, Arc<AtomicBool>)>> = Mutex::new(Vec::new());
