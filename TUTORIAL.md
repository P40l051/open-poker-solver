# Tutorial — Using OPS

A practical guide to the postflop solver. Everything runs locally: no data
leaves your computer.

## Contents

1. [What it does and what it does not do](#1-what-it-does-and-what-it-does-not-do)
2. [Starting the app](#2-starting-the-app)
3. [Conventions](#3-conventions)
4. [Inputs](#4-inputs)
5. [GUI step by step](#5-gui-step-by-step)
6. [CLI step by step](#6-cli-step-by-step)
7. [Typical workflows](#7-typical-workflows)
8. [Accuracy and timings](#8-accuracy-and-timings)
9. [Common errors and issues](#9-common-errors-and-issues)
10. [Glossary](#10-glossary)
11. [FAQ](#11-faq)

OPS is open-source software released under the MIT License. The source code and
downloads are at <https://github.com/P40l051/open-poker-solver>.

---

## 1. What it does and what it does not do

**It does**

- Solve a heads-up postflop spot with vector CFR+ over all 1326 combos.
- Exact river and turn solves (all runouts); sampled runouts on the flop.
- Compute the average strategy, EV per player, best response and exploitability.
- Import PokerStars hands and derive board, pot, stack, sizes and rake.
- Show the strategy tree, per-class strategy matrix and per-combo drill-down.

**It does not**

- Invent ranges: one hand history is not enough. You choose the ranges.
- Handle multiway (3+ players): it reduces to heads-up with a warning.
- Model ICM, side pots, multiway antes or progressive rake.
- Act as a preflop GTO trainer: it starts from the flop, turn or river.

Rule of thumb: the solver answers “given these two ranges and this spot, what are
the optimal frequencies?”.

---

## 2. Starting the app

From a checkout:

```bash
cargo build --release --offline      # first time only / after changes
./target/release/ops-gui             # GUI on http://127.0.0.1:7788
./target/release/ops --help          # CLI
```

From the packaged app: double-click **OPS.app**. The app starts a local server
and opens a Chrome app window. If the window does not open, visit
`http://127.0.0.1:7788/`.

The **Guide** link in the header reopens this page. **Glossary** opens a
searchable list of poker and solver terms.

Stop the server with **Quit** in the header, `Ctrl+C` in the terminal, or:

```bash
lsof -ti tcp:7788 | xargs kill
```

If port 7788 is busy, OPS uses 7789, 7790, and so on.

---

## 3. Conventions

| Concept | Meaning |
|---|---|
| **bb** | big blind. Import converts everything to bb: pot 20 = 20 bb. |
| **Pot** | chips already in the middle when the solved street starts. |
| **Stack** | effective stack *behind* (the smaller of the two). |
| **OOP / IP** | out of position (acts first) / in position. |
| **Sizing** | pot fraction: `0.75` = 75% pot; commas for multiple sizes. |
| **Solved street** | 5-card board → river only; 4 → turn + river; 3 → flop + turn + river. |

EVs are in chips (bb). `+2.50` means +2.50 bb relative to the start of the spot.

---

## 4. Inputs

### 4.1 Board

`AhKd7c` (flop), `AhKd7c2s` (turn), `AhKd7c2s9h` (river). Case-insensitive,
spaces ignored. Duplicate cards are an error.

The solver starts **at the beginning of the matching street**: a 4-card board
solves turn betting and then the river. It does not jump to the river.

You can also use the **Deck** button: click cards from the full 52-card deck.
The first three selected cards form the flop, then turn, then river. Click a
selected card (or a slot) to remove it; **OK** applies the board.

### 4.2 Ranges

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

Commas separate groups. Example:

```
22+,A2s+,K9s+,QTs+,JTs,ATo+,KQo
```

Ranges are normalised **after** board card removal: if `AA` has three combos
left because an ace is on the board, those three combos are 100% of the range.
This is correct and different from “3/6 = 50%”.

Use **Grid** next to each range to edit it on a 13×13 matrix:

- diagonal = pairs
- above the diagonal = suited
- below the diagonal = offsuit
- click or drag to select/deselect; the footer shows combos, % of all hands and
  the number of hand classes
- **Clear**, **All**, **Cancel**, **OK**
- the matrix writes compact syntax back into the text field (`QQ+`,
  `A2s+`, `A9s-A2s`, `random`)

Imported hero cards are marked and excluded from the board picker.

### 4.3 Sizing

- `--sizes 0.33,0.75` applies to every street.
- In the GUI: separate Flop / Turn / River fields.
- The first solved street uses the matching field.
- Raises are sized on the pot after the call (`raise to`), with a cap.
- `max_raises` limits raises per street; all-in is always available.
- Every extra size multiplies nodes: two sizes cost roughly 3–4×.

### 4.4 Runouts

- River/turn: `runout_samples = 0` → exact enumeration (default).
- Flop: the default samples 7 turns × 6 rivers. Increase with
  `--turn-samples`, `--runouts` for more precision (linear cost).
- With sampling the strategy is noisy: use at least 300–500 iterations.

### 4.5 Rake

`rake = min(pct × pot, cap)`, applied to folds and showdowns. With rake the game
is constant-sum: `EV_OOP + EV_IP = −rake paid`. The rake observed in an imported
hand is filled automatically.

### 4.6 Iterations

- River: 200–400 is enough (often < 0.1% pot exploitability).
- Turn: 800–2000.
- Sampled flop: 300–600 (accuracy is limited by the sample).
- `target_exploitability` stops earlier when the threshold is reached.

---

## 5. GUI step by step

### 5.1 Import a hand

1. Paste a PokerStars hand history into **Import hand**.
2. Press **Import hand**.
3. OPS fills: board, pot (bb), stack (bb), observed sizes, rake %, hero cards.
4. The **Preflop line** box shows the original raiser, the last preflop
   aggressor, open size, callers and pot type: limped, SRP, 3BP or 4BP+.
5. Notes and warnings appear below:
   - grey **notes** (rake, unusual sizing, 3-bet roles) — informative;
   - orange **warnings** (multiway, hero not in hand, opener folded) — real
     limits.
6. If a starter chart is available, press **Use starter ranges** to fill OOP and
   IP. It is a heuristic, not a solved GTO range: edit it before solving.

Example (real hand):

```
street       river (Qs 2d Th Td 6c)
pot          2.07 chips = 103.50 bb
stack        0.64 chips = 32.00 bb
players      OOP cobra.it57 / IP Pekoz051
positions    OOP BB / IP BTN
preflop      BTN opens 5.00 bb, BB/HJ calls -> SRP
roles        preflop raiser BTN (Pekoz051)
sizes        1% pot
rake         5.7%
hero         Pekoz051 [As Ac]
```

A 1% size is the real bet (€0.02 into €2.07): add `0.75` next to it for a useful
tree.

In a 3-bet pot the two roles are different, for example:

```
preflop      CO opens 2.50 bb, BTN 3-bets -> 3BP
roles        opener CO (CO) / preflop aggressor BTN (BTN)
```

The original raiser is the first raise; the preflop aggressor is the last raise.
Do not assign the opener range to the 3-bettor.

### 5.2 Enter ranges

Ranges cannot be derived from a single hand. A plausible example for the spot
above:

- OOP (BB defending vs a raise): `22+,A2s+,K9s+,QTs+,JTs,ATo+,KQo`
- IP (BTN who raised): `TT+,AQs+,AKo`

If a range is fully blocked by the board you get
`a range has no combo compatible with the board`.

You can type the shorthand or use **Grid** for each range. The starter-range
button is only a starting point: for a serious study replace it with the ranges
you believe are correct for the players and the preflop line.

### 5.3 Run the solve

Press **Solve**. The bar shows iteration and exploitability. **Stop** cancels
and keeps the partial result.

### 5.4 Read the result

- **EV OOP / EV IP** — expected chips from the start of the spot.
- **Exploitability** — how much can be gained by deviating. `< 0.5% pot` is
  good; `< 0.1%` is excellent. It drops with more iterations.
- **Chart** — exploitability per iteration; it should go down.
- **Action legend** — semantic colors: Check green, Bet blue, Fold orange, Call
  purple, Raise yellow.
- **Strategy tree** — see the next section.
- **Hand detail** — click a node.

### 5.5 Strategy tree

Three views, toggled with **Table | Tree | Graph**:

- **Table** — one row per decision node: street, board, actor, pot, frequency
  bar. Hover the bar for exact percentages.
- **Tree** — indented tree with percentages on every action:
  - expand a node (`▸`) to see its actions;
  - expand an action to see the next decision node;
  - chance cards appear on the branch (`7h →`);
  - **end** means no further decision (fold or showdown).
- **Graph** — node-link diagram of the same solution:
  - each node shows street, board, actor, pot and a strategy bar;
  - every action edge is labelled with its frequency and colored by action;
  - chance nodes are collapsed into one box (`7 cards ▸`) — click it and type
    (e.g. `7h`) or pick **one** card: only that runout branch opens, so the
    diagram stays readable; add more cards one at a time if you want;
  - click **+** on a node to expand it, click a node to open its hand detail;
  - drag to pan, **Ctrl+scroll** to zoom, **Fit** recentres, **Expand root** /
    **Collapse all** reset the view, the search box jumps to a node by `#id`;
  - the graph is lazy (only expanded nodes are drawn), so it stays responsive
    on flops with thousands of decision nodes.

Every row is a decision node of the **same** solution. It is not a separate
solution. A river spot with one bet size usually has only four decision nodes:

1. OOP: check or bet
2. IP after check: check or bet
3. OOP facing a bet: fold or call
4. IP facing a bet: fold or call

### 5.6 Hand detail: combos and classes

Click a node:

- **Combos** — each combo that reaches the node, its range weight and the
  frequency of every action. The imported hero hand is highlighted with a
  **hero** badge.
- **Classes** — the same data aggregated into the 169 hand classes:
  - each cell is a stacked color bar; mixed strategies show exact proportions;
  - tooltip: combos in the class, range weight, % per action;
  - click a class to filter the combo list; the chip `×` removes the filter;
  - classes not in range are dashed;
  - the hero class has a green ring.

A node like `check 62% / bet 15 38%` means that, averaged over the range, the
line is check 62% and bet 38%. To see *which hands* bet, open the combo or class
detail.

Colors always use the legend: Check green, Bet blue, Fold orange, Call purple,
Raise yellow. Percentages are the share of the acting range using an action, not
win probabilities.

---

## 6. CLI step by step

### 6.1 Manual spot

```bash
./ops \
  --board AhKd7c2s9h \
  --oop "QQ+,AKs,AQo" --ip "JJ+,AQs,KJs" \
  --pot 20 --stack 80 \
  --sizes 0.75 --rake-pct 5 \
  --iterations 400
```

### 6.2 From a PokerStars hand

```bash
./ops --hand-file hand.txt
# prints the derived spot and the preflop line, then exits

./ops --hand-file hand.txt --prefill
# uses the OPS starter chart and solves

./ops --hand-file hand.txt \
  --oop "22+,A2s+,K9s+,QTs+,JTs,ATo+" \
  --ip "TT+,AQs+,AKo" \
  --iterations 400
```

The import overrides board/pot/stack/sizes; ranges remain yours. Without
`--oop/--ip`, OPS prints the starter ranges (if the spot is supported);
`--prefill` uses them. Starter ranges are heuristic: edit them for a serious
study.

### 6.3 JSON for scripts

```bash
./ops --board AhKd7c2s9h --oop AA --ip KK \
  --json --out sol.json
```

It contains `config`, `summary` (EV, BR, exploitability, history) and `nodes`
with aggregated frequencies plus tree edges.

### 6.4 Useful options

| Flag | Effect |
|---|---|
| `--sizes`, `--turn-sizes`, `--river-sizes` | sizes per street |
| `--raises N` | maximum raises per street |
| `--runouts N` | sampled rivers (0 = all) |
| `--turn-samples N` | sampled turns on a flop |
| `--rake-pct`, `--rake-cap` | rake |
| `--iterations`, `--target-expl` | budget / early stop |
| `--max-nodes` | tree size guard |
| `--quiet` | no progress on stderr |

---

## 7. Typical workflows

### 7.1 River review

1. Import a hand → the spot is the river.
2. Realistic ranges for both players.
3. `iterations 300`, observed size plus one standard size.
4. Compare your line with the node frequencies; open the combo detail on your
   hand.

### 7.2 Turn

A 4-card board. The solver plays turn and river. Higher cost: use 800–1500
iterations and 1–2 sizes per street.

### 7.3 Flop

A 3-card board. Sampled by default (7×6). Good for range advantage and macro
lines, not for decimal frequencies.

### 7.4 Study an exploit

Raise iterations and watch best response / exploitability. If it stays high,
the average strategy is not exploiting the opponent much (or the sample is
noisy).

### 7.5 Compare sizing

The same spot with `--river-sizes 0.33` vs `0.75` vs `1.25`: compare EV OOP. The
best size is the one with the highest EV for the player choosing it, with the
same ranges.

---

## 8. Accuracy and timings

Real measurements on an Apple Silicon Mac (release build):

| Spot | Iterations | Exploitability | Time |
|---|---|---|---|
| River AA vs KK | 200 | 0.01% pot | 0.1 s |
| River, real ranges | 400 | < 0.05% pot | 0.2 s |
| Turn, real ranges | 1200 | ~0.9% pot | ~60 s |
| Sampled flop 7×6 | 200 | ~2.7% pot | ~30 s |

- River: almost exact, the main use case.
- Turn: slower convergence; more iterations help.
- Flop: approximation controlled by the sample, not by the maths.
- On turn/flop the two EVs come from distinct marginals: they may differ by
  ~1% of the pot. Strategies are not affected.

---

## 9. Common errors and issues

| Message | Cause | Fix |
|---|---|---|
| `a range has no combo compatible with the board` | the whole range is blocked by the board | widen the range |
| `tree exceeds max_nodes` | too many sizes/raises/streets | reduce sizes or raise `--max-nodes` |
| `the hand ended preflop: no postflop spot` | hand ended preflop | use another hand |
| `no postflop betting spot found` | no voluntary postflop action | check the hand |
| `could not read the blinds` | header is not PokerStars | unsupported format |
| `4 players active on the turn…` | multiway | reduced to heads-up; verify the two names |
| `hero … is not in the hand` | hero folded | cards are informational only |
| `preflop opener … folded before the flop` | the original raiser is not in the solved pair | ranges refer to the remaining players; do not use opener ranges |
| `no starter ranges for this spot` | opener/aggressor not in the heads-up pair | pass `--oop/--ip` manually |
| `no rake recorded …` | rake rounded to zero | set Rake % manually |
| `observed bet is 1% of the pot` | unusual bet in the hand | add a normal size |

If the GUI does not respond, check `curl http://127.0.0.1:7788/api/health`.
If the port changed, check the URL printed by the app.

On macOS, if the app is blocked on first launch: right-click → **Open**, or:

```bash
xattr -dr com.apple.quarantine /Applications/OPS.app
```

---

## 10. Glossary

- **CFR+**: self-play algorithm that minimises regret; converges to Nash.
- **Regret matching**: probabilities proportional to positive regrets.
- **Infoset**: public node + private combo.
- **Counterfactual value**: value of a combo assuming it always reaches the node.
- **Best response (BR)**: best reply to the opponent's average strategy.
- **Exploitability**: average gain of two best responses; distance from
  equilibrium.
- **Range**: probability distribution over the 1326 combos.
- **Card removal / blocker**: cards in your hand or on the board exclude
  opponent combos.
- **Runout**: community cards still to be dealt (turn/river).
- **Constant-sum**: the sum of EVs is constant (−rake), not zero.
- **Node**: one decision point in the tree.
- **Frequency**: share of the acting range using an action at a node.

The in-app **Glossary** button contains the full searchable version.

---

## 11. FAQ

**Why is EV IP not exactly −EV OOP?**
With rake the game is constant-sum. On turn/flop the two EVs come from distinct
marginals and may diverge by ~1% of the pot.

**Can I use weighted ranges?**
Yes: `QQ+:0.5` or `AA:0.3,random`.

**Why does the solver check so much?**
It is often the optimal line; the range average hides strong hands that bet.
Open the combo/class detail.

**Can I solve 3 players?**
No. Import reduces to heads-up with a warning; multiway needs a different
engine.

**How many iterations to publish a spot?**
River: 400 with exploitability < 0.1% pot. Turn: ≥ 1500. Flop: always state that
it is sampled.

**Is the flop reliable?**
For macro lines yes; for precise frequencies no. Increase samples or use a
full-flop solver.

**Why do percentages not add up to exactly 100%?**
Rounding in the UI. The underlying frequencies sum to 1.

**What does “end” mean in the tree?**
The action leads to a terminal state: fold or showdown. There is no further
decision.
