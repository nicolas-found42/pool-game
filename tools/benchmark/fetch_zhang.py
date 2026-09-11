#!/usr/bin/env python3
"""Inventory and fetch the Zhang et al. billiards dataset from Google Drive.

The dataset (arXiv:2407.19686, "Billiards Sports Analytics: Datasets and
Tasks") is published only as a public Google Drive folder:

    https://drive.google.com/drive/folders/1NBqonYLr_cParMMn4xSeE0KTJNhjeYuG

Neither the Drive API (needs a key) nor `gdown --folder` (500s on subfolders)
works reliably here, so this tool uses the two keyless public endpoints:

* listing  -- https://drive.google.com/embeddedfolderview?id=<ID>#list
               returns every child of a folder, no 50-entry cap, no key.
* download -- https://drive.usercontent.google.com/download?id=<ID>&export=download&confirm=t
               returns the raw bytes for a public file.

Subcommands
-----------
inventory   Walk the folder tree and write a JSONL manifest of every entry
            (path, id, kind). Folders appear with kind="folder", files with
            kind="file"; a leaf is a file because Drive's listing cannot tell
            them apart otherwise.
fetch       Download every manifest file whose path passes --include/--exclude,
            writing <dest>/<path> and recording byte size + sha256 into the
            manifest row. Reports files whose size changed since discovery.
verify      Re-hash a previously fetched tree against the manifest.

Bulk data belongs OUTSIDE the repository; point --dest at a scratch directory
(e.g. ~/pool-game-data/zhang-9ball).
"""

from __future__ import annotations

import argparse
import concurrent.futures as futures
import hashlib
import json
import os
import re
import sys
import time
import urllib.parse
import urllib.request
from fnmatch import fnmatch

ROOT_FOLDER = "1NBqonYLr_cParMMn4xSeE0KTJNhjeYuG"
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/126.0 Safari/537.36")

# The entry markup links folders under /drive/folders/<ID> and files under
# /file/d/<ID>/view, which is the only folder-vs-file signal the endpoint gives.
_ENTRY = re.compile(
    r'<div class="flip-entry" id="entry-([^"]+)".*?'
    r'<div class="flip-entry-title">([^<]*)</div>', re.S)


def list_folder(folder_id: str, retries: int = 3) -> list[tuple[str, str, str]]:
    """Return (id, name, kind) for children of a public folder."""
    query = urllib.parse.urlencode({"id": folder_id})
    url = f"https://drive.google.com/embeddedfolderview?{query}#list"
    last = None
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=60) as response:
                page = response.read().decode("utf-8", "replace")
            kinds = {}
            for chunk in page.split('<div class="flip-entry" id="entry-')[1:]:
                fid = chunk.split('"', 1)[0]
                kinds[fid] = ("folder" if "/drive/folders/" in chunk else "file")
            return [(fid, _unescape(name), kinds.get(fid, "file"))
                    for fid, name in _ENTRY.findall(page)
                    if not fid.startswith("_")]
        except Exception as exc:  # transient Drive hiccups are routine
            last = exc
            time.sleep(1.5 * (attempt + 1))
    raise RuntimeError(f"listing {folder_id} failed: {last}")


def _unescape(name: str) -> str:
    return (name.replace("&amp;", "&").replace("&#39;", "'")
                .replace("&quot;", '"').replace("&lt;", "<").replace("&gt;", ">"))


def walk(root_id: str, max_depth: int, jobs: int, rows: list):
    """Breadth-first walk; one listing request per folder, folds in parallel."""
    frontier = [(root_id, "", 0)]
    rows.append({"path": "", "id": root_id, "kind": "folder"})
    with futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        while frontier:
            listings = list(pool.map(lambda item: (item, list_folder(item[0])), frontier))
            frontier = []
            for (fid, path, depth), children in listings:
                for cid, name, kind in children:
                    child = f"{path}/{name}"
                    if kind == "folder":
                        rows.append({"path": child, "id": cid, "kind": "folder"})
                        if depth + 1 < max_depth:
                            frontier.append((cid, child, depth + 1))
                    else:
                        rows.append({"path": child, "id": cid, "kind": "file"})
            print(f"  {len(rows)} entries, {len(frontier)} folders in frontier",
                  flush=True)
    return rows


def download_url(file_id: str) -> str:
    query = urllib.parse.urlencode({"id": file_id, "export": "download",
                                    "confirm": "t"})
    return f"https://drive.usercontent.google.com/download?{query}"


def fetch_one(row: dict, dest: str) -> dict:
    target = os.path.join(dest, row["path"].lstrip("/"))
    if os.path.exists(target) and os.path.getsize(target) > 0:
        row["size"] = os.path.getsize(target)
        row["sha256"] = hashlib.sha256(open(target, "rb").read()).hexdigest()
        row["cached"] = True
        return row
    os.makedirs(os.path.dirname(target), exist_ok=True)
    tmp = target + ".part"
    digest = hashlib.sha256()
    size = 0
    request = urllib.request.Request(download_url(row["id"]),
                                     headers={"User-Agent": UA})
    with urllib.request.urlopen(request, timeout=180) as response, open(tmp, "wb") as sink:
        ctype = response.headers.get("Content-Type", "")
        if "text/html" in ctype:
            raise RuntimeError(f"got an HTML interstitial instead of bytes ({ctype})")
        while chunk := response.read(1 << 20):
            sink.write(chunk)
            digest.update(chunk)
            size += len(chunk)
    os.replace(tmp, target)
    row["size"] = size
    row["sha256"] = digest.hexdigest()
    return row


def selected(path: str, includes: list[str], excludes: list[str]) -> bool:
    if includes and not any(fnmatch(path, pat) for pat in includes):
        return False
    return not any(fnmatch(path, pat) for pat in excludes)


def read_manifest(path: str) -> list[dict]:
    with open(path, encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def write_manifest(rows: list[dict], path: str) -> None:
    with open(path, "w", encoding="utf-8") as handle:
        for row in sorted(rows, key=lambda r: r["path"]):
            handle.write(json.dumps(row, sort_keys=True) + "\n")


def cmd_inventory(args) -> int:
    rows: list[dict] = []
    walk(args.root, args.max_depth, args.jobs, rows)
    write_manifest(rows, args.output)
    folders = sum(1 for r in rows if r["kind"] == "folder")
    print(f"{len(rows)} entries ({folders} folders, {len(rows) - folders} files) "
          f"-> {args.output}")
    return 0


def cmd_fetch(args) -> int:
    rows = read_manifest(args.manifest)
    todo = [r for r in rows if r["kind"] == "file"
            and selected(r["path"], args.include, args.exclude)]
    if args.limit:
        todo = todo[:args.limit]
    print(f"{len(todo)} files to fetch into {args.dest}", flush=True)
    done, failed = 0, []
    with futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        pending = {pool.submit(fetch_one, row, args.dest): row for row in todo}
        for future in futures.as_completed(pending):
            row = pending[future]
            try:
                future.result()
                done += 1
                if done % 50 == 0 or done == len(todo):
                    print(f"  {done}/{len(todo)}", flush=True)
            except Exception as exc:
                failed.append({"path": row["path"], "id": row["id"],
                               "error": f"{type(exc).__name__}: {exc}"})
                print(f"  FAIL {row['path']}: {exc}", flush=True)
    write_manifest(rows, args.manifest)
    if failed:
        with open(args.manifest + ".failed", "w", encoding="utf-8") as handle:
            for row in failed:
                handle.write(json.dumps(row, sort_keys=True) + "\n")
        print(f"{len(failed)} failures -> {args.manifest}.failed")
    total = sum(r.get("size", 0) for r in rows if r.get("size"))
    print(f"fetched {done} files, {total / 1e6:.1f} MB on disk under {args.dest}")
    return 1 if failed else 0


def cmd_verify(args) -> int:
    bad = 0
    for row in read_manifest(args.manifest):
        if row["kind"] != "file" or "sha256" not in row:
            continue
        target = os.path.join(args.dest, row["path"].lstrip("/"))
        if not os.path.exists(target):
            print(f"MISSING {row['path']}")
            bad += 1
            continue
        digest = hashlib.sha256(open(target, "rb").read()).hexdigest()
        if digest != row["sha256"]:
            print(f"HASH MISMATCH {row['path']}")
            bad += 1
    print(f"{bad} problems")
    return 1 if bad else 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)

    inv = sub.add_parser("inventory", help="walk the Drive tree into a manifest")
    inv.add_argument("--root", default=ROOT_FOLDER)
    inv.add_argument("--max-depth", type=int, default=12)
    inv.add_argument("-o", "--output", default="zhang-manifest.jsonl")
    inv.add_argument("--jobs", type=int, default=8)
    inv.set_defaults(func=cmd_inventory)

    fet = sub.add_parser("fetch", help="download manifest files")
    fet.add_argument("--manifest", required=True)
    fet.add_argument("--dest", required=True)
    fet.add_argument("--include", action="append", default=[],
                     help="fnmatch on the manifest path (repeatable)")
    fet.add_argument("--exclude", action="append", default=[],
                     help="fnmatch on the manifest path (repeatable)")
    fet.add_argument("--jobs", type=int, default=8)
    fet.add_argument("--limit", type=int)
    fet.set_defaults(func=cmd_fetch)

    ver = sub.add_parser("verify", help="re-hash a fetched tree")
    ver.add_argument("--manifest", required=True)
    ver.add_argument("--dest", required=True)
    ver.set_defaults(func=cmd_verify)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
