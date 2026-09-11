# Benchmark data: what was acquired, and what it can and cannot support

Status: provenance and inventory note for wayfinder ticket #15 (AFK task, no gate). It feeds the fitting ladder of the physics section (#7 §6) and the prototype ticket #8; ticket #12 folds the "supports / does not support" lines into the assembled spec. Bulk datasets stay outside the repository — every path below is absolute and outside the checkout.

Machine all measurements were taken on: Apple M5, macOS 26.4 (Darwin 25.4.0, arm64), Python 3.14.7 (system) and 3.12.14 (pooltool venv), 664 GB free at the start.

| Source | What it is | Verdict |
|---|---|---|
| Zhang et al. 9-ball pro set (arXiv:2407.19686) | Kinovea-annotated break layouts and ball tracks from pro 9-ball broadcasts | **Acquired**: 1.4 GB, 10,247 files, loader works |
| Published curves (Dr. Dave TPs, Shepard, Platinum) | Throw, draw/follow, squirt, bank and rail data | **Digitized**: 4 files in `data/curves/`, provenance and self-checks inside |
| Rodriguez-Lozano "Billiard-dataset" (Appl. Intell. 2023) | 300 recordings, ~190 GB, CC BY-SA 4.0 | **Not obtainable**: SharePoint share requires Microsoft sign-in |
| pooltool (JOSS 10.21105/joss.07301) | Independent billiards simulator | **Runs** here; version and transcript below |

## 1. Zhang et al. 9-ball pro set

**Where.** The only publication route is a public Google Drive folder,
<https://drive.google.com/drive/folders/1NBqonYLr_cParMMn4xSeE0KTJNhjeYuG> ("Billiards Sports"), linked from the
dataset paper "Billiards Sports Analytics: Datasets and Tasks" (arXiv:2407.19686) and its SIGMOD-style supplement
(<https://zhengwang125.github.io/paper/SDM_Supplementary.pdf>). Both were downloaded with the data and are quoted below.

**What was actually downloaded.**

| Path | Contents | Size |
|---|---|---|
| `~/pool-game-data/zhang-9ball/files/` | the whole folder, minus 3 `.mp4` reference videos and 68 `.DS_Store` | **1,445.8 MB**, 10,247 files |
| `~/pool-game-data/zhang-9ball/files/code/` | authors' code: 17 `.py`, 2 `.ipynb`, `README.md`, one PDF | 528 KB |
| `~/pool-game-data/zhang-9ball/files/data _trajectories/` | 6 games' worth of Kinovea ball tracks | 60 MB, 3,100 files |
| `~/pool-game-data/zhang-9ball/files/data_layouts/` | every game's break layouts, variables and key images | 1.3 GB, 7,127 files |
| `~/pool-game-data/zhang-9ball/manifest.jsonl` | full tree listing with Drive ids, sizes and sha256 per fetched file | — |
| `~/pool-game-data/zhang-9ball/sample/Frame 1.xml` | one layout file kept outside the tree while writing the loader | 9 KB |
| `~/pool-game-data/sources/` | the source PDFs and pages the curve files were transcribed from | 4.2 MB |

File types in the download: 5,001 `.xml` (SpreadsheetML), 1,318 `.xlsx`, 2,496 `.png`, 1,411 `.jpg`/`.jpeg`, 20 code/text files. The images are the Kinovea "key image" screenshots stored beside each layout; they are not needed for the fits.

**Structure and encoding.**

    code/{BLCNN,BLGAN,Preprocessing}/…              authors' notebooks (no dataset licence file)
    data_layouts/All cordinates/<game>/<match>/Frame N.xml     one layout per frame
    data_layouts/Variables/<game>/<match>/Variables.xlsx       per-match metadata + YouTube URL
    data _trajectories/<game>[/<match>]/trajectory/round N/object K/
        cue ball.xml        Kinovea track of the cue ball
        object K.xml        Kinovea track of object ball K
        attribute.xlsx      the cue-stick annotation for that strike

The `.xml` files are **not XML in the usual sense**: they are SpreadsheetML 2003 workbooks (`urn:schemas-microsoft-com:office:spreadsheet`), i.e. Excel 2003 XML exports. A layout file holds a "Points" table of ten (`Marker 1` … `Marker 10`, x, y) rows; a track file holds `Label :`, the literal header row `Coords (x,y:cm; t:time)` and then `x, y, t` rows with `t` written as `MM:SS.hh`. `attribute.xlsx` is a two-column key/value sheet: `game`, `player`, `view` (`V`/`H`), `cushion`, `intersection distance`, `stick top position`, `angle(°)` — the cue-stick line as drawn in Kinovea, which is the closest thing in the set to a strike declaration. `Variables.xlsx` carries the match header (game, match, YouTube link) and one row per frame: `Who BREAK`, `Potted when break`, `Potted after break`, `Clear`, `Win`, `Horizontal or Vertical`, `Remarks`, `turns`, `order`, `fouls`, `type of foul`, `view`, `cushion`.

**Per-strike fields, and what the counts really mean.** The paper's headline "2,082 strikes with trajectories" is the number of exported **track XML files**, not 2,082 distinct shots. The download contains:

| Level | Count |
|---|---|
| games with layout data | 97 (the paper says 94; the folder split differs) |
| layout files (`Frame N.xml`) | 2,869, holding 28,690 marker rows |
| games with trajectory data | 6 |
| tracked rounds (`round N`) | **180** |
| (cue ball, object ball) tracking pairs (`object K`) | **1,006** |
| track XML files | **2,082** |
| track samples | 143,922 |
| `attribute.xlsx` strike annotations | 1,004 |

So a runnable end-to-end corpus is **180 rounds / 1,006 tracked shots**, each with a cue-ball track, one or more object-ball tracks, and (usually) one cue-line annotation — not 2,082 shots.

**Coordinate frame and scale.** The paper states the processed frame precisely (supplement §1.2): the playing field is mapped into a **200 × 100 grid with the bottom-left corner as the origin**, x ∈ [0, 200], y ∈ [0, 100], obtained from a Kinovea perspective grid, with the layouts rotated so that every game is in the horizontal viewing angle. That normalisation is **not present in the raw files**, and the raw files are what the download contains. Measured over all 28,690 layout markers and 143,922 track samples:

- the long axis is **y in 64 of 94 games** and x in 30 — the raw tree mixes orientations, so a consumer must rotate per game;
- coordinates run x ∈ [−232.0, 208.5], y ∈ [−59.0, 214.0] for layouts and x ∈ [−13.5, 323.9], y ∈ [−285.5, 433.9] for tracks, i.e. well outside the nominal box;
- per-game spans of the marker cloud range from 125.6 to 419.7 units (median 195.2, p25 185.5, p75 201.1). Assuming a 9 ft 2540 mm playing length, that is a median 13.0 mm per unit — but the spread means **the annotators' grid scale is not consistent between games** and a fit must normalise per game against a known table dimension rather than assume 200 units = 2540 mm.

There is **no unit in the files**: the track header says `(x,y:cm)`, but the numbers are Kinovea grid units, not centimetres — a 200-unit table cannot be 200 cm long. Treat the label as wrong.

**Documented annotation noise.** The paper documents **none**: it says the tracks come from manual Kinovea markers, that "we randomly choose some of games to double-check the accuracy", and gives no error figure, repeatability, or inter-annotator study. `load_zhang.py inventory` therefore reports measured proxies:

- 13.7% of layout markers have a negative coordinate, i.e. the marker sits outside the annotator's nominal origin;
- 40.0% of layout markers fall outside the nominal [0, 200] × [0, 100] box;
- track sampling is ~33 Hz (median gap 0.03 s, p10–p90 0.03–0.04 s), median 41 samples per track, so most tracks are 1–2 s long;
- 4.4% of consecutive samples are bit-identical to their predecessor (marker staleness: the annotator did not move the marker between frames).

**Terms of use.** The paper states the dataset is "publicly available without copyright restrictions, offering an easily accessible opportunity for both research and commercial use" (arXiv:2407.19686 §3). No `LICENSE`/`COPYING` file ships with the download (checked). The underlying material is broadcast video from YouTube, mirrored into `Variables.xlsx` as links; three `.mp4` reference videos in `Variables/` were deliberately not downloaded. Record the paper's statement, not an SPDX id — there is no SPDX licence to point at.

**Loader.** `tools/benchmark/load_zhang.py` reads the real files with `xml.etree` and `zipfile` only (no dependencies), see §5.

## 2. The published curves (`data/curves/`)

Each file is a single JSON document with `sources`, `method`, `parameters`, `error_estimate` and the points. Two acquisition routes are used and labelled separately:

- **closed-form evaluation** where the source publishes an equation and its parameters (throw, draw/follow, rail travel) — no pixel reading, no digitization error, and the script re-checks the source's own printed anchors on every run;
- **verbatim transcription** where the source publishes a table (the throw calibration, the Platinum shaft table, the through-diamond bank measurements).

| File | Contents | Gate it serves |
|---|---|---|
| `throw.json` | 2,280 model points (cut 0–75°, speeds 0.5/1.5/4.5 m/s, roll 0/50%, english −100…+100%), max-throw and gearing summaries, plus the 6 measured TP B-3 calibration points with model residuals | ladder stage 2 |
| `draw-follow.json` | 120 draw-distance points over cue speed × pre-impact distance × tip offset, the rolling-direct-hit validation table, TP B-5's printed anchors, and the 1/8 rule recorded as qualitative only | ladder stage 1 |
| `cushion-bank.json` | the TP B-6 rail-travel curve (0.5–20 mph), the printed anchors with residuals, the WPA 4–4.5 table-length acceptance test, and the five tables' measured through-diamond bank data | ladder stage 3 |
| `squirt.json` | the 46-shaft Platinum table with derived squirt angles, Shepard's pivot-point/endmass map, and the published ranges | ladder stage 4 |

Method notes that matter when gating:

- `throw.json` is the evaluation of TP A-28's MathCAD formulation of TP A-14 Eqs. 15–17 —
  `atan( min( μ(v_rel)·v·cos φ / v_rel, 1/7 ) · (v·sin φ − R·ω_z) / (v·cos φ) )` with `μ(v) = a + b·e^(−c·v)`
  and `ω = (5/4)(v/R)·PE` — at the document's own parameters. The equation was read from the PDF's positioned
  glyphs, not from a rendered image.
- Its `error_estimate` carries the model-vs-measurement residual: TP B-3's own refit of the friction model
  under-predicts its own softest 30° point by 2.88° (its worst residual; the 45° column agrees within 0.72°,
  mean residual −0.73°). TP B-3's printed
  degree column is also inconsistent with `atan(inches_per_yard / 36)` by ~0.6–1% (a MathCAD unit slip); use the
  inches-per-yard column.
- `cushion-bank.json` reproduces TP B-6's printed table lengths exactly at 1.5 and 3 mph and comes out 1.5–2.4%
  short at 7/12/20 mph, so the source's own numbers disagree with its own algorithm at speed; treat them as ±2%.
- `draw-follow.json` reproduces TP B-5's hop heights and times exactly but **not** its printed travel ratio:
  the printed constants (μ_b 0.06, e_b 0.94) give d_OB/d_CB = 7.56 where the document prints 6.08. The prototype
  should re-derive the constant from the relation rather than trust the ratio.
- `squirt.json`'s two sources disagree (Shepard 0.5–2.3° over ~10–50 in pivots; Platinum 1.3–2.3° over 7.6–14.1 in),
  so the stage-4 band is the union until a cue is chosen.
- Nothing was traced by eye from a plot. Mathavan 2010's rebound results are plots with no table, so they were
  **not** digitized; the spec's existing warning against copying its model-internal e_n = 0.98 / μ = 0.14 as an
  effective rail COR still stands, and TP B-6's measured e_c = 0.7 is the usable rail figure.

## 3. Rodriguez-Lozano "Billiard-dataset" — not obtainable here

The repository <https://github.com/FJ-Rodriguez-Lozano/Billiard-dataset> is CC BY-SA 4.0 (confirmed through the
GitHub API: `license.spdx_id = CC-BY-SA-4.0`) and describes 300 recordings (100 blackball, 100 carom, 100 snooker)
with manually obtained trajectories for a 22/16/16 subset, ~190 GB, hosted on a personal SharePoint share:

    https://ucordoba-my.sharepoint.com/:f:/g/personal/i02rolof_uco_es/EqvYzy9bx_hNjAOfptG03jMBEoBLwBQZRkCMdmM31IW_5w?e=3aa7hU

**It is not practical to fetch in this environment**, and this is the fallback the campaign anticipated. Evidence:

- `curl -L` on the share URL ends at `https://login.microsoftonline.com/…/oauth2/authorize?…` with page id
  `BssoInterrupt` — the share resolves into a Microsoft sign-in, not into content;
- the OneDrive REST endpoint for the share returns `HTTP 401` and
  `{"error":{"code":"unauthenticated", … "@onedrive.linkFeatures":[]}}` — the link carries **no anonymous link
  features**, so no keyless API route exists;
- `rclone` (v1.75.1, installed) has no configured remote and cannot build a filesystem from the link alone:
  `unable to get drive_id and drive_type`.

Disk space is not the blocker (664 GB free against ~190 GB); the blocker is an interactive Microsoft account
sign-in plus a multi-hour, ~190 GB transfer. **Consequence for the ladder: the end-to-end gate (stage 5) rests on
the Zhang set alone**, at 180 rounds / 1,006 tracked shots rather than 2,082. If the dataset is wanted later, the
route is a browser sign-in whose session cookies are handed to a downloader, run outside the repo.

## 4. pooltool — the differential oracle

Installed into `~/pool-game-data/pooltool-venv` (Python 3.12.14) from PyPI package **`pooltool-billiards`**, not
`pooltool` (that name belongs to an unrelated BitShares liquidity-pool tool). Command and observed output on this
machine:

    $ uv venv ~/pool-game-data/pooltool-venv --python 3.12
    $ VIRTUAL_ENV=~/pool-game-data/pooltool-venv uv pip install pooltool-billiards
    $ ~/pool-game-data/pooltool-venv/bin/python tools/benchmark/pooltool_smoke.py

    pooltool version: 0.6.0
    python: 3.12.14
    table: pocket playing surface 990.6 x 1981.2 mm
    cushion height: 36.576 mm (64.00 % of ball diameter)

    === shot straight_stun ===  (event list abridged in this document; 30 events total)
    ball params: R=28.575 mm m=170.1 g u_s=0.2 u_r=0.01 u_sp_proportionality=0.4444444444444444 u_b=0.05 e_b=0.95 e_c=0.85 f_c=0.2 g=9.81
    strike declaration: {"V0": 1.4, "phi": 90.0, "theta": 0.0, "a": 0.0, "b": 0.0}  object-ball lateral offset: 0.000 mm
    simulated duration: 4.9542 s, events: 30
      t= 0.000000s  none                     ['dummy']
      t= 0.000000s  stick_ball               ['cue_stick', 'cue']
      t= 0.068457s  ball_ball                ['cue', '1']
      t= 0.092540s  sliding_rolling          ['cue']
      t= 0.372662s  sliding_rolling          ['1']
      t= 0.632926s  ball_linear_cushion      ['1', '9']
      t= 0.813509s  sliding_rolling          ['1']
      t= 1.088847s  rolling_stationary       ['cue']
      ... 18 more
    rest state:
      cue: pos=( 495.300,  579.699, 28.575) mm  |v|=0.000000 m/s  |w|=0.000000 rad/s  state=0
        1: pos=( 495.300, 1036.030, 28.575) mm  |v|=0.000000 m/s  |w|=0.000000 rad/s  state=0

    === shot half_ball_follow ===  (straight-in aim, object ball offset one ball radius = 30 deg cut, b = 0.5 R follow)
    strike declaration: {"V0": 1.4, "phi": 90.0, "theta": 0.0, "a": 0.0, "b": 0.5}  object-ball lateral offset: 28.575 mm
    simulated duration: 4.7601 s, events: 21
    first ball-ball at t=0.066262s; geometry-only OB cut angle = 30.0000 deg
    observed OB departure = 29.1670 deg (cut-induced throw = -0.8330 deg)
    observed CB departure = -55.8384 deg (tangent line = 90 deg for a stun, less for follow)
    rest state:
      cue: pos=( 350.220, 1514.707, 28.575) mm  |v|=0.000000 m/s  |w|=0.000000 rad/s  state=0
        1: pos=( 951.168, 1934.227, 28.575) mm  |v|=0.000000 m/s  |w|=0.000000 rad/s  state=0

Everything needed was observed: the engine simulates to rest, the straight stun stops the cue ball dead (0.000000 m/s),
and the half-ball cut produces a **cut-induced throw of 0.83°** with the object ball departing below the geometric
30°, plus the expected tangent-line deviation under follow. Four things the differential stage must know:

- **pooltool's default table is a 7 ft Showood** (990.6 × 1981.2 mm), not the spec's 9 ft playing surface;
  `--table nine-foot` builds 1270 × 2540 mm from `PocketTableSpecs(l=2.540, w=1.270)` for a like-for-like diff;
- its cushion height is 64.00% of ball diameter (the spec pins 63.5%);
- its defaults differ from the spec's profile: `u_b` 0.05 (spec 0.06), `e_c` 0.85 (spec default 0.75),
  `g` 9.81 (spec 9.80665); the spec's values must be pushed in explicitly for a meaningful comparison;
- at 30°, 1.4 m/s, the measured throw (0.83°) sits **below** both published throw models evaluated for the same
  shot (TP A-28 friction: 1.75°; TP B-3 friction: 4.72°). That gap is exactly the sort of thing the pre-fit
  differential stage exists to surface, and it should be settled before stage 2's gate is trusted.

Note the `PoolTool` wheel on PyPI (v2.1.2, "Liquidity Pools on the BitShares blockchain") is a name collision, and
that the import takes ~45 s the first time because numba compiles.

## 5. Running the tools

Everything under `tools/benchmark/` is pure Python 3 (standard library only) except the pooltool script, which
needs the pooltool venv.

    # 1. re-list the Drive folder (no key needed; ~11k entries, ~5 min)
    python3 tools/benchmark/fetch_zhang.py inventory -o ~/pool-game-data/zhang-9ball/manifest.jsonl

    # 2. download (resumable: existing files are hashed and skipped; 503s are recorded, re-run to retry)
    python3 tools/benchmark/fetch_zhang.py fetch \
        --manifest ~/pool-game-data/zhang-9ball/manifest.jsonl \
        --dest ~/pool-game-data/zhang-9ball/files \
        --exclude '*.mp4' --exclude '*.DS_Store' --jobs 12

    # 3. counts, coordinate ranges, and the measured annotation-noise proxies
    python3 tools/benchmark/load_zhang.py inventory --root ~/pool-game-data/zhang-9ball/files

    # 4. per-strike records (add --game to filter, --limit to bound)
    python3 tools/benchmark/load_zhang.py strikes --root ~/pool-game-data/zhang-9ball/files --limit 1

    # 5. regenerate the curve files (self-checks print the source residuals)
    python3 tools/benchmark/gen_curves.py --out data/curves

    # 6. the differential oracle
    ~/pool-game-data/pooltool-venv/bin/python tools/benchmark/pooltool_smoke.py [--table nine-foot] [--csv /tmp/cue.csv]

`load_zhang.py layout <file>` and `load_zhang.py track <file>` print single files, which is how the encodings above
were established.

## 6. Supports / does not support, against the fitting ladder (#7 §6)

| Stage | Zhang set | Curves | pooltool |
|---|---|---|---|
| 1. long straight stop/draw/follow | **Partially.** Tracks give cue-ball paths and rest positions; no per-shot spin or cue speed is recorded, so μs/μr cannot be fitted from it alone. The six `attribute.xlsx` cue fields give aim, not speed. | **Supports.** `draw-follow.json` gives the draw/follow relation and its cloth sensitivity; TP B-2's μr ≈ 0.01 and spin-down ≈ 10 rad/s² measured on cloth are the stage-1 prior. | **Supports.** Deterministic replay at any profile; use for parameter sweeps before fitting. |
| 2. cuts at known angles (throw) | **Does not support.** No cut angle, ball-ball contact geometry, or spin is annotated; trajectories are recorded at ~33 Hz which is far too coarse for the 2 ms contact. | **Supports.** `throw.json` gives throw vs cut angle, speed, english and roll plus the measured calibration points; max throw ≈ 5° and the gearing rule are both in the file. | **Partially.** Produces throw (0.83° at 30°/1.4 m/s) but its default μ_b = 0.05 disagrees with both curve models; treat as a cross-check, not a reference. |
| 3. bank grid with english | **Partially.** Tracks cross cushions, so rebound geometry is observable; no spin annotation. Only 6 games have tracks. | **Supports.** `cushion-bank.json` holds rail COR (e_c = 0.7), the speed-vs-travel relation, the WPA acceptance test, and five tables' measured bank data. | **Supports.** Event types include both cushion types; visible bank/kick diffs. |
| 4. pivot-length / squirt | **Does not support.** No cue or shaft is identified per shot, and no tip offset is recorded (`stick top position` is the stick line, not the tip contact). | **Supports.** `squirt.json` gives 46 measured shafts plus the pivot↔endmass mapping. | **Partially.** Squirt is modelled through cue `end_mass`/specs; no measured cue is shipped, so it validates the mechanism, not a cue. |
| 5. full-shot replay (end-to-end gate) | **Partially, and this is the headline limitation.** 180 rounds / 1,006 tracked shots with rest positions and outcomes, in a per-game grid frame with no published scale. It supports outcome agreement and rest-position deviation *after per-game normalisation*, on ~1,000 shots — not the 2,082 the paper implies. | **Not applicable.** | **Supports.** Differential replay of identical shots; the pre-fit stage the ladder asks for. |
| break acceptance hook | **Does not support.** Break layouts exist (2,869 frames) but the rack-to-rack propagation is not annotated. | **Not applicable.** | **Supports.** Rack generation and break simulation are built in. |

Two consequences the spec must carry: the end-to-end corpus is ~1,000 shots from 6 tournaments (not 2,082 from 94), and **no open dataset in hand provides spin, cue speed, or cue identification**, so stages 2–4 are curve-gated only — there is no measured spin ground truth to fit against.
