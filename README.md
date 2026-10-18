# OPS — Open Poker Solver

[![MIT License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Open Source](https://img.shields.io/badge/open%20source-yes-brightgreen.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20universal-blue.svg)](#download)
[![Release](https://img.shields.io/github/v/release/P40l051/open-poker-solver?label=release)](https://github.com/P40l051/open-poker-solver/releases)

A local postflop CFR+ solver for heads-up no-limit hold'em cash games, with a
desktop UI, PokerStars hand import and rake support.

OPS is **free and open-source software** released under the [MIT License](LICENSE).
You can use it, study it, modify it and redistribute it.

---

## Download

**Latest release: v0.3.0 — macOS universal (Apple Silicon + Intel)**

[**Download OPS-0.3.0-macos-universal.dmg**](https://github.com/P40l051/open-poker-solver/releases/latest/download/OPS-0.3.0-macos-universal.dmg)

SHA-256 checksum:
[`OPS-0.3.0-macos-universal.dmg.sha256`](https://github.com/P40l051/open-poker-solver/releases/latest/download/OPS-0.3.0-macos-universal.dmg.sha256)

1. Double-click the DMG.
2. Drag **OPS** into **Applications**.
3. First launch: right-click the app → **Open** (the build is ad-hoc signed, not
   notarized). Alternatively:
   ```bash
   xattr -dr com.apple.quarantine /Applications/OPS.app
   ```

No installer, no account, no runtime dependencies. Windows and Linux builds are
compiled from source — see [Distribution](#7-distribution).

---

## Contents

- [What OPS does](#1-what-ops-does)
- [What OPS does not do](#2-what-ops-does-not-do)
- [Architecture](#3-architecture)
- [Using the desktop app](#4-using-the-desktop-app)
- [Range syntax](#5-range-syntax)
- [Command line](#6-command-line)
- [Distribution](#7-distribution)
- [Declared limits](#8-declared-limits)
- [Verification](#9-verification)
- [Next steps](#10-next-steps)
- [License](#license)

OPS is written from scratch in Rust with no external dependencies. It solves the
public betting tree with vector CFR+ over all 1326 combos, evaluates 7-card hands
exactly at showdown, handles card removal between both ranges, and reports the
average strategy, best responses and exploitability.

Everything runs on your machine: the UI is served from `127.0.0.1` and no data is
sent anywhere.

```
cargo build --release
./target/release/ops --board AhKd7c2s9h --oop "QQ+,AKs,AQo" --ip "JJ+,AQs,KJs" \
    --pot 20 --stack 80 --iterations 300
./target/release/ops-gui            # desktop app on 127.0.0.1:7788
```

---

## 1. What OPS does

- Solves a heads-up postflop spot with vector CFR+ over all 1326 combos.
- Exact river and turn solves (all runouts); sampled runouts on the flop.
- Average strategy, EV per player, exact best response and exploitability.
- Public betting tree with multiple bet sizes, raises, all-in and rake.
- PokerStars hand import: board, pot, effective stack, observed sizes, rake,
  hero cards, OOP/IP players, positions.
- Preflop line classifier: original raiser, last preflop aggressor, open size,
  callers and pot type (limped, SRP, 3-bet, 4-bet+).
- Optional starter-range prefill for the imported spot. The chart is a heuristic
  starting point, always editable and never hidden from the solver.
- Desktop UI with deck picker, 13×13 range grid, strategy tree, per-class
  strategy matrix, combo drill-down and a built-in glossary.
- CLI with JSON output for scripting and batch use.

## 2. What OPS does not do

- It does not invent solved ranges. A single hand history cannot tell you both
  ranges; you provide them. The optional prefill is a heuristic starter chart,
  clearly labelled and meant to be edited.
- No multiway (3+ players). Imported multiway hands are reduced to heads-up with
  an explicit warning.
- No ICM, side pots, multiway antes or progressive rake.
- No preflop solver: OPS starts from the flop, turn or river.

Practical rule: the solver answers “given these two ranges and this spot, what
are the optimal action frequencies?”.

---

## 3. Architecture

```
src/
  cards.rs   cards, combos 0..1326, board parsing
  range.rs   range parser (AA, TT+, A2s+, T9s-76s, weights :0.5)
  eval.rs    7-card hand evaluator (category + kickers in a u32)
  tree.rs    public betting tree, sizing, raise cap, all-in, chance
  cfr.rs     vector CFR+, best response, exploitability
  json.rs    minimal JSON parser/writer
  hh.rs      PokerStars hand history parser + preflop classifier
  presets.rs starter preflop charts for imported spots
  report.rs  text/JSON reports
  lib.rs     configuration and orchestration
  bin/ops.rs       CLI
  bin/ops_gui.rs   local HTTP server + embedded UI
assets/index.html  single-page UI (zero dependencies)
```

### Game model

- Two players; OOP acts first on every postflop street.
- Actions: check, bet (N sizes), fold, call, raise (configurable cap) and
  automatic all-in when a size exceeds the stack.
- Initial pot and effective stack in chips (1 unit = 1 bb if you prefer).
- Terminal states: fold or exact 7-card showdown.
- Chance: turn and river are public chance nodes. River exact by default, turn
  exact, flop sampled (`turn_samples`, `runout_samples`).

### CFR

- Regret matching+ with a floor at zero, alternating updates (one player per
  traversal), linearly weighted average strategy.
- Per-infoset state: `(public node, combo)`, tables of `actions × 1326` `f32`
  cells for regrets and strategy sums.
- Vector reach for both players; counterfactual values are computed for every
  combo in one pass.
- Showdown in `O(combos)`: combos sorted by strength, histograms per card and
  strength group, exact correction for blocked cards between the two ranges.
- Chance: weight `1/(n-2)` per card (combo mass stays 1) and a `(n-2)/(n-4)`
  rescale of branch values to restore the joint probability.
- Exploitability = `(BR_OOP + BR_IP) / 2`, using an exact best response against
  the average strategy.

### Measured accuracy (test spots)

| Spot | Iterations | Exploitability |
|---|---|---|
| River `AhKd7c2s9h`, AA vs KK | 200 | 0.01% pot |
| River, real ranges | 400 | < 0.05% pot |
| Turn `AhKd7c2s`, real ranges | 1200 | ~0.9% pot |
| Sampled flop (7 turns × 6 rivers) | 200 | ~2.7% pot |

River converges in a fraction of a second; turn needs more iterations; flop is
approximate by construction.

---

## 4. Using the desktop app

### 4.1 Start

Double-click **OPS.app**, or from a terminal:

```bash
./ops-gui              # opens a Chrome app window on 127.0.0.1:7788
./ops-gui --no-open    # server only; open the URL manually
```

If port 7788 is busy, OPS tries 7789, 7790, and so on. **Quit** in the header
stops the local server.

Requirements: macOS 11+ (universal build). Chrome is optional: if it is missing,
the default browser is opened.

### 4.2 Spot fields

- **Import hand** — paste one PokerStars hand history and press **Import hand**.
  OPS fills the board of the analysed street, pot and effective stack in bb,
  observed bet sizes, rake, hero cards and the OOP/IP players. It also shows the
  preflop line: original raiser, preflop aggressor, open size and pot type
  (limped / SRP / 3BP / 4BP+). If a starter chart is available, **Use starter
  ranges** fills the OOP and IP boxes; edit them before solving.
- **Board** — 3, 4 or 5 cards. Use the **Deck** button for a 52-card picker.
  The first three cards form the flop, then turn, then river.
- **OOP range / IP range** — the hands each player can have. OOP acts first.
  Use **Grid** to edit them on a 13×13 matrix: diagonal = pairs, above = suited,
  below = offsuit. Hero cards are excluded from the board picker after import.
- **Pot / Effective stack** — chips at the start of the solved street.
- **Rake % / Rake cap** — percentage of the final pot and optional chip cap.
- **Sizes** — bet sizes per street as pot fractions (`0.75` = 75% pot). Commas
  allow multiple sizes (`0.33,0.75,1.25`).
- **Max raises** — raise branches allowed per street.
- **River samples** — flop approximation. `0` is exact on turn and river.
- **Iterations** — refinement budget; more is more accurate and slower.

Every field has a `?` help icon: hover for a short explanation, click to open the
matching glossary entry. The header **Glossary** button opens the searchable
glossary directly.

### 4.3 Solve and read the result

Press **Solve**. The progress bar shows iterations and exploitability; **Stop**
keeps the partial result.

- **EV OOP / EV IP** — expected chips from the start of the spot.
- **Exploitability** — chips an opponent could gain by deviating. Under 0.5% of
  the pot is good; under 0.1% is excellent.
- **% of pot** — exploitability relative to the pot, useful to compare spots.
- **Chart** — exploitability per iteration; it should go down.
- **Action legend** — semantic colors: Check green, Bet blue, Fold orange,
  Call purple, Raise yellow. Colors show the share of the range using each
  action.

### 4.4 Strategy tree

The **Strategy tree** panel has two views:

- **Table** — one row per decision node with street, board, actor, pot and a
  stacked frequency bar.
- **Tree** — indented tree: expand a node (`▸`) to see its actions with
  percentages, then expand an action to see the next decision node. Chance
  cards appear on the branch (`7h →`). An action with no further decision shows
  **end**.

Important: every row is a decision node of the **same** solution, not a separate
solution. A river spot with one bet size typically has only four decision nodes.

### 4.5 Hand detail

Click a node (table row or tree row) to open the hand detail:

- **Combos** — every combo that reaches the node, its range weight and the
  frequency of each action. The imported hero hand is highlighted.
- **Classes** — the same data aggregated into the 169 hand classes. Each cell is
  a stacked color bar, so a mixed strategy shows the exact proportions. Click a
  class to filter the combo list.

Percentages are the share of the acting range using an action at that node, not
win probabilities.

---

## 5. Range syntax

| Syntax | Meaning |
|---|---|
| `AA`, `KK` | exact pair |
| `TT+` | TT, JJ, QQ, KK, AA |
| `AKs` / `AKo` / `AK` | suited / offsuit / both |
| `A2s+` | A2s…AKs (ace fixed) |
| `T9s+` | T9s, JTs, QJs, KQs, AKs (constant gap) |
| `A5s-A2s`, `TT-77` | ranges |
| `random` | all 1326 combos |
| `QQ+:0.5` | 50% weight on QQ+ |
| `AA:0.3,random` | mixed weights |

Commas separate groups:

```
22+,A2s+,K9s+,QTs+,JTs,ATo+,KQo
```

Ranges are normalised **after** board card removal. If `AA` has three combos
left because an ace is on the board, those three combos are treated as 100% of
the range. This is correct and different from “3/6 = 50%”.

---

## 6. Command line

```bash
./ops --board AhKd7c2s9h \
      --oop "QQ+,AKs,AQo,KQs" --ip "JJ+,AQs,KJs,QJs" \
      --pot 20 --stack 80 \
      --sizes 0.75 --turn-sizes 0.75 --river-sizes 0.5,1.0 \
      --iterations 500 --json --out solution.json
```

Main options: `--raises`, `--runouts`, `--turn-samples`, `--target-expl`,
`--check-every`, `--max-nodes`, `--quiet`.

`--json` produces the configuration, summary (EV, best responses,
exploitability, history) and all decision nodes with aggregated frequencies and
tree edges.

### Import a PokerStars hand

```bash
./ops --hand-file hand.txt                # show the derived spot + preflop line
./ops --hand-file hand.txt --prefill      # use the starter chart and solve
./ops --hand-file hand.txt --oop "..." --ip "..." --iterations 300
```

The import derives the analysed board, pot and effective stack in bb, the two
players, the sizes actually used and hero cards. It also classifies the preflop
line: original raiser (first raise), last preflop aggressor (last raise), open
size, callers, positions and pot type. In a 3-bet pot those two players are
different, so OPS shows both.

Ranges cannot be derived from a single hand. Without `--oop/--ip`, OPS prints the
available starter ranges; `--prefill` uses them. They are a heuristic chart, not
a solved range: edit them before treating the result as a serious study.

---

## 7. Distribution

### macOS DMG

The release script builds a universal app (arm64 + x86_64), signs it ad-hoc and
creates a DMG with a drag-to-Applications layout:

```bash
./scripts/package.sh        # app, CLI and archives
./scripts/package_dmg.sh    # universal app + DMG
```

Output: `dist/OPS-<version>-macos-universal.dmg`, containing:

- `OPS.app`
- `Applications` shortcut
- `README.md`
- `TUTORIAL.md`

The DMG is ad-hoc signed, not notarized. On first launch: right-click the app →
**Open**, or remove quarantine:

```bash
xattr -dr com.apple.quarantine /Applications/OPS.app
```

For public distribution, sign with a Developer ID, notarize and staple the DMG;
see [`DISTRIBUTION.md`](DISTRIBUTION.md).

### Windows and Linux

The app bundle is macOS-only. Windows and Linux builds are compiled on their
target OS:

```powershell
cargo build --release
```

Ship `ops.exe` and `ops-gui.exe` in a folder with the documentation. The GUI
opens the default browser. Add a launcher or an Inno Setup installer for
non-technical users.

---

## 8. Declared limits

- Starter ranges are a heuristic chart for common 6-max cash spots. They are not
  solved GTO ranges and they do not replace a preflop solution.
- Rake is a percentage of the pot with an optional cap. Side pots and ICM are
  not modelled.
- With public runouts (turn/flop) the two EVs come from distinct marginals and
  may differ by ~1% of the pot. Strategies are not affected.
- The flop samples runouts: the strategy is approximate and noisy with few
  samples. Increase `--turn-samples` / `--runouts` (linear cost).
- Chance probability is handled in vector form: the per-infoset strategy is
  exact, the EV with sampled runouts is an estimate.
- No board bucketing: cost grows with the number of solved streets.
- Linear average-strategy weight; no pruning or CFR-D.
- `f32` regrets and strategy sums: practical accuracy, not extreme precision.
- Solutions live in memory; saving and loading solution files is not implemented
  yet.

## 9. Verification

```bash
cargo test --offline
./target/release/ops --board AhKd7c2s9h --oop AA --ip KK --iterations 200
./target/release/ops-gui --no-open --port 7790 &
curl http://127.0.0.1:7790/api/health
```

## 10. Next steps

1. Prune combos with zero reach and parallelise chance branches.
2. Board/range bucketing for full flop solves.
3. CFR-D / DCFR with tuning and on-disk checkpoints.
4. Solution save/load and a persistent spot library.
5. Node locking and re-solving from an arbitrary node.

## 11. Open source

OPS is **open-source software** distributed under the [MIT License](LICENSE).
You are free to use, copy, modify, merge, publish, distribute, sublicense and
sell copies of the software, provided the copyright notice and permission notice
are included.

- Source code: <https://github.com/P40l051/open-poker-solver>
- Issues and feature requests:
  <https://github.com/P40l051/open-poker-solver/issues>
- Step-by-step guide: [TUTORIAL.md](TUTORIAL.md)

The software is provided “as is”, without warranty of any kind. It is a study
and analysis tool, not gambling advice.

## License

MIT — see [LICENSE](LICENSE).
