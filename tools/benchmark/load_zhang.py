#!/usr/bin/env python3
"""Load the Zhang et al. 9-ball dataset (arXiv:2407.19686) as per-strike records.

The download itself is fetched by ``fetch_zhang.py``; see
``docs/spec/benchmark-data.md`` for what the folder actually contains. Files are
SpreadsheetML 2003 workbooks (``.xml``, a Kinovea "export coordinates" dump) and
plain ``.xlsx``; neither needs a third-party library, so this loader reads them
with ``xml.etree`` and ``zipfile`` only.

Dataset layout (paths are relative to the download root)::

    code/...                                              authors' notebooks
    data_layouts/All cordinates/<game>/<match>/Frame N.xml   break-shot layouts
    data_layouts/Variables/<game ...>/...                    per-game metadata
    data _trajectories/<game>/trajectory/round N/object K/
        cue ball.xml     Kinovea track of the cue ball
        object K.xml     Kinovea track of object ball K
        attribute.xlsx   cue-stick annotation for the strike

Subcommands
-----------
inventory  Walk a download root and report counts plus the coordinate ranges
           actually present (which is how the frame/orientation is settled).
strikes    Print one record per strike: cue annotation, tracked balls, sample
           counts, sampling interval, coordinate ranges.
layout     Print a single layout file.
track      Print a single track file.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import re
import statistics
import sys
import zipfile
from xml.etree import ElementTree

SS = "{urn:schemas-microsoft-com:office:spreadsheet}"
XLSX_NS = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"


# --------------------------------------------------------------------------- #
# file readers
# --------------------------------------------------------------------------- #
def read_spreadsheetml(path):
    """Return the rows of the first worksheet as lists of cell values."""
    root = ElementTree.parse(path).getroot()
    worksheet = root.find(f"{SS}Worksheet")
    if worksheet is None:
        raise ValueError(f"{path}: no <Worksheet>")
    name = worksheet.get(f"{SS}Name", "")
    rows = []
    for row in worksheet.iter(f"{SS}Row"):
        cells = []
        for cell in row.iter(f"{SS}Cell"):
            index = cell.get(f"{SS}Index")
            if index is not None:                      # sparse: pad skipped cells
                while len(cells) < int(index) - 1:
                    cells.append(None)
            data = cell.find(f"{SS}Data")
            if data is None or data.text is None:
                cells.append(None)
                continue
            kind = data.get(f"{SS}Type")
            text = data.text.strip()
            if kind == "Number":
                cells.append(float(text))
            else:
                cells.append(text)
        rows.append(cells)
    return name, rows


def read_xlsx(path):
    """Return the first sheet of an .xlsx as a list of row lists.

    Handles the subset Excel actually emits here: shared strings plus inline
    numbers, with the cell reference giving the column index.
    """
    with zipfile.ZipFile(path) as archive:
        shared = []
        if "xl/sharedStrings.xml" in archive.namelist():
            root = ElementTree.fromstring(archive.read("xl/sharedStrings.xml"))
            for item in root.iter(f"{XLSX_NS}si"):
                shared.append("".join(node.text or "" for node in item.iter(f"{XLSX_NS}t")))
        sheet_name = "xl/worksheets/sheet1.xml"
        root = ElementTree.fromstring(archive.read(sheet_name))

    rows = []
    for row in root.iter(f"{XLSX_NS}row"):
        cells = []
        for cell in row.iter(f"{XLSX_NS}c"):
            ref = cell.get("r") or ""
            column = 0
            for char in re.match(r"[A-Z]*", ref).group(0):
                column = column * 26 + (ord(char) - 64)
            while len(cells) < column - 1:
                cells.append(None)
            value = cell.find(f"{XLSX_NS}v")
            if value is None or value.text is None:
                cells.append(None)
                continue
            if cell.get("t") == "s":
                cells.append(shared[int(value.text)])
            else:
                cells.append(float(value.text))
        rows.append(cells)
    return rows


def parse_clock(text):
    """Kinovea writes ``MM:SS.hh`` (sometimes ``HH:MM:SS.hh``); return seconds."""
    parts = str(text).split(":")
    try:
        numbers = [float(part) for part in parts]
    except ValueError:
        return None
    seconds = 0.0
    for number in numbers:
        seconds = seconds * 60 + number
    return seconds


def parse_layout(path):
    """A break-shot layout: the ten markers with their (x, y) and timestamp."""
    name, rows = read_spreadsheetml(path)
    markers = []
    for row in rows:
        if len(row) >= 3 and isinstance(row[0], str) and row[0].startswith("Marker"):
            x = row[1] if isinstance(row[1], float) else None
            y = row[2] if isinstance(row[2], float) else None
            markers.append((row[0], x, y))
    return {"sheet": name, "markers": markers, "rows": rows}


def parse_track(path):
    """A Kinovea track: the label plus (x, y, t) samples."""
    name, rows = read_spreadsheetml(path)
    label = ""
    samples = []
    for row in rows:
        if len(row) >= 2 and row[0] == "Label :":
            label = str(row[1])
        elif len(row) >= 3 and row[0] == "x":
            continue
        elif len(row) >= 3 and isinstance(row[0], float) and isinstance(row[1], float):
            seconds = parse_clock(row[2]) if len(row) > 2 and row[2] else None
            samples.append((row[0], row[1], seconds))
    return {"sheet": name, "label": label, "samples": samples}


def parse_attribute(path):
    """The cue-stick annotation shipped beside each tracked strike."""
    rows = read_xlsx(path)
    return {str(row[0]): row[1] for row in rows
            if len(row) >= 2 and row[0] is not None}


# --------------------------------------------------------------------------- #
# strike assembly
# --------------------------------------------------------------------------- #
ROUND = re.compile(r"round (\d+)$")
OBJECT = re.compile(r"object (\d+)$")


def iter_strikes(root):
    """Yield one record per ``.../trajectory/round N`` directory.

    The tree is not uniform: some games are ``<game>/trajectory/round N`` and
    others ``<game>/<match>/trajectory/round N``, so match on the whole subtree.

    Within a round each ``object K`` folder is one Kinovea tracking *pair*: the
    cue ball's track plus the track of object ball K, exported separately. A
    round therefore yields several pairs, and the cue ball is re-tracked in each
    one -- the per-round record keeps them all rather than deduplicating, which
    would silently drop tracks.
    """
    pattern = os.path.join(root, "data _trajectories", "**", "trajectory", "round *")
    marker = os.sep + "data _trajectories" + os.sep
    for round_dir in sorted(glob.glob(pattern, recursive=True)):
        parts = round_dir.split(marker, 1)[-1].split(os.sep)
        game = parts[0]
        match = parts[-3] if len(parts) >= 4 else game   # 2-level trees have no match
        number = ROUND.search(os.path.basename(round_dir))
        record = {
            "game": game,
            "match": match,
            "round": int(number.group(1)) if number else None,
            "dir": round_dir,
            "pairs": {},
        }
        for object_dir in sorted(glob.glob(os.path.join(round_dir, "object *"))):
            ball_name = os.path.basename(object_dir)
            pair = {"cue": None, "objects": {}, "attributes": None}
            attribute = os.path.join(object_dir, "attribute.xlsx")
            if os.path.exists(attribute):
                pair["attributes"] = parse_attribute(attribute)
            for track_file in glob.glob(os.path.join(object_dir, "*.xml")):
                stem = os.path.splitext(os.path.basename(track_file))[0]
                track = parse_track(track_file)
                if stem == "cue ball":
                    pair["cue"] = track
                else:
                    pair["objects"][stem] = track
            record["pairs"][ball_name] = pair
        yield record


def _track_summary(track):
    if track is None:
        return None
    samples = track["samples"]
    timed = [s for s in samples if s[2] is not None]
    gaps = [b[2] - a[2] for a, b in zip(timed, timed[1:]) if b[2] > a[2]]
    return {
        "label": track["label"],
        "samples": len(samples),
        "start_s": timed[0][2] if timed else None,
        "duration_s": round(timed[-1][2] - timed[0][2], 4) if timed else None,
        "median_gap_s": statistics.median(gaps) if gaps else None,
        "x_range": [min(s[0] for s in samples), max(s[0] for s in samples)]
                   if samples else None,
        "y_range": [min(s[1] for s in samples), max(s[1] for s in samples)]
                   if samples else None,
    }


def summarise(record):
    out = {
        "game": record["game"],
        "match": record["match"],
        "round": record["round"],
        "pairs": {},
    }
    for name, pair in record["pairs"].items():
        out["pairs"][name] = {
            "cue": _track_summary(pair["cue"]),
            "objects": {stem: _track_summary(track)
                        for stem, track in pair["objects"].items()},
            "attributes": pair["attributes"],
        }
    return out


# --------------------------------------------------------------------------- #
# subcommands
# --------------------------------------------------------------------------- #
def cmd_inventory(args):
    counts = {"layouts": 0, "layout_markers": 0, "games": set(),
              "rounds": 0, "tracks": 0, "attribute_files": 0, "sample_rows": 0}
    x_all, y_all = [], []
    per_game = {}
    layout_pattern = os.path.join(args.root, "data_layouts", "All cordinates", "*", "*",
                                  "*.xml")
    for path in sorted(glob.glob(layout_pattern)):
        layout = parse_layout(path)
        counts["layouts"] += 1
        counts["layout_markers"] += len(layout["markers"])
        # <root>/data_layouts/All cordinates/<game>/<match>/<file>.xml
        parts = path.split(os.sep)
        game = parts[-3]
        counts["games"].add(game)
        stats = per_game.setdefault(game, {"x": [], "y": [], "matches": set()})
        stats["matches"].add(parts[-2])
        for _, x, y in layout["markers"]:
            if x is not None:
                x_all.append(x)
                stats["x"].append(x)
            if y is not None:
                y_all.append(y)
                stats["y"].append(y)

    frame = []
    for game, stats in sorted(per_game.items()):
        if not stats["x"] or not stats["y"]:
            continue
        dx = max(stats["x"]) - min(stats["x"])
        dy = max(stats["y"]) - min(stats["y"])
        long_axis = "x" if dx >= dy else "y"
        span = max(dx, dy)
        frame.append({
            "game": game,
            "matches": len(stats["matches"]),
            "x_range": [round(min(stats["x"]), 2), round(max(stats["x"]), 2)],
            "y_range": [round(min(stats["y"]), 2), round(max(stats["y"]), 2)],
            "long_axis": long_axis,
            "span_units": round(span, 2),
            "mm_per_unit_9ft": round(2540.0 / span, 4) if span else None,
        })

    track_x, track_y = [], []
    samples_per_track = []
    gaps = []
    stale = 0
    adjacent = 0
    pairs = 0
    for record in iter_strikes(args.root):
        counts["rounds"] += 1
        counts["games"].add(record["game"])
        for pair in record["pairs"].values():
            pairs += 1
            if pair["attributes"]:
                counts["attribute_files"] += 1
            for track in [pair["cue"], *pair["objects"].values()]:
                if track is None:
                    continue
                counts["tracks"] += 1
                counts["sample_rows"] += len(track["samples"])
                samples_per_track.append(len(track["samples"]))
                times = [s[2] for s in track["samples"] if s[2] is not None]
                gaps.extend(b - a for a, b in zip(times, times[1:]) if b > a)
                for first, second in zip(track["samples"], track["samples"][1:]):
                    adjacent += 1
                    stale += 1 if (first[0] == second[0] and first[1] == second[1]) else 0
                for x, y, _ in track["samples"]:
                    track_x.append(x)
                    track_y.append(y)

    # The paper documents no quantitative annotation error, so measure proxies.
    off_table = sum(1 for x, y in zip(x_all, y_all) if x < 0 or y < 0)
    over_range = sum(1 for x, y in zip(x_all, y_all) if x > 200 or y > 100.0)
    gaps.sort()
    report = {
        "root": os.path.abspath(args.root),
        "games_with_layouts": len(counts["games"]),
        "layout_files": counts["layouts"],
        "layout_markers": counts["layout_markers"],
        "strike_rounds": counts["rounds"],
        "tracking_pairs": pairs,
        "track_files": counts["tracks"],
        "track_samples": counts["sample_rows"],
        "strike_attribute_files": counts["attribute_files"],
        "layout_x_range": [min(x_all), max(x_all)] if x_all else None,
        "layout_y_range": [min(y_all), max(y_all)] if y_all else None,
        "track_x_range": [min(track_x), max(track_x)] if track_x else None,
        "track_y_range": [min(track_y), max(track_y)] if track_y else None,
        "annotation_noise": {
            "note": "the paper documents no error figure; these are measured proxies",
            "layout_markers_negative": off_table,
            "layout_markers_negative_fraction": round(off_table / len(x_all), 5) if x_all else None,
            "layout_markers_outside_200x100": over_range,
            "layout_markers_outside_fraction": round(over_range / len(x_all), 5) if x_all else None,
            "track_samples_median_per_track": (statistics.median(samples_per_track)
                                               if samples_per_track else None),
            "track_median_gap_s": (round(statistics.median(gaps), 5) if gaps else None),
            "track_gap_p10_p90_s": ([round(gaps[len(gaps) // 10], 4),
                                     round(gaps[9 * len(gaps) // 10], 4)] if gaps else None),
            "track_repeated_sample_fraction": (round(stale / adjacent, 5)
                                               if adjacent else None),
        },
        "per_game_frame": frame,
    }
    if args.game_summary:
        report["per_game_frame"] = [row for row in frame if args.game_summary.lower()
                                    in row["game"].lower()]
    print(json.dumps(report, indent=2))
    return 0


def cmd_strikes(args):
    shown = 0
    for record in iter_strikes(args.root):
        if args.game and args.game.lower() not in record["game"].lower():
            continue
        print(json.dumps(summarise(record), indent=2, default=str))
        shown += 1
        if shown >= args.limit:
            break
    print(f"# {shown} strike records shown", file=sys.stderr)
    return 0


def cmd_layout(args):
    layout = parse_layout(args.path)
    print(f"sheet: {layout['sheet']}")
    for name, x, y in layout["markers"]:
        print(f"  {name:<10} x={x:8.2f} y={y:8.2f}")
    return 0


def cmd_track(args):
    track = parse_track(args.path)
    print(f"sheet: {track['sheet']}  label: {track['label']}  "
          f"samples: {len(track['samples'])}")
    for x, y, t in track["samples"][:args.limit]:
        print(f"  t={t if t is None else round(t, 2)!s:>8}  x={x:8.2f}  y={y:8.2f}")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)

    inv = sub.add_parser("inventory", help="counts and coordinate ranges")
    inv.add_argument("--root", required=True)
    inv.add_argument("--game-summary", help="only this game in the per-game frame table")
    inv.set_defaults(func=cmd_inventory)

    strikes = sub.add_parser("strikes", help="one record per strike")
    strikes.add_argument("--root", required=True)
    strikes.add_argument("--limit", type=int, default=5)
    strikes.add_argument("--game", help="substring filter on the game (folder) name")
    strikes.set_defaults(func=cmd_strikes)

    layout = sub.add_parser("layout", help="print one layout file")
    layout.add_argument("path")
    layout.set_defaults(func=cmd_layout)

    track = sub.add_parser("track", help="print one track file")
    track.add_argument("path")
    track.add_argument("--limit", type=int, default=10)
    track.set_defaults(func=cmd_track)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
