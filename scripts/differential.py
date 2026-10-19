"""Differential test on real files: read every .DS_Store under the given folders with dsxray and with
the Python `ds_store` library, and compare every record (name, code, type, raw value).

Real .DS_Store files hold real file names, so this prints only counts and record codes, never a
name, a path or a value. Run it on your own Mac; the files themselves stay out of the repository.

    cargo build --release
    uv run --with ds_store python scripts/differential.py target/release/dsxray ~/Desktop ~/Documents
"""
import json
import os
import subprocess
import sys
from collections import Counter

import ds_store.store as store
from ds_store import DSStore

store.codecs.clear()  # keep blobs as raw bytes, the way dsxray reports them


def python_records(path):
    with DSStore.open(path, "r") as d:
        out = []
        for e in d:
            v = e.value
            if isinstance(v, (bytes, bytearray)):
                v = v.hex() if e.type == b"blob" else v.decode("latin-1")
            elif isinstance(v, bool):
                v = bool(v)
            out.append((e.filename, e.code.decode("latin-1"), e.type.decode("latin-1"), v))
        return out


def dsxray_records(binary, path):
    run = subprocess.run([binary, "--json", path], capture_output=True, text=True)
    doc = json.loads(run.stdout)
    if "error" in doc:
        raise ValueError(doc["error"])
    store_ = doc["store"]
    if store_["problem"]:
        raise ValueError(store_["problem"])
    return [(r["name"], r["code"], r["type"], r["value"]) for r in store_["records"]]


def files(roots):
    for root in roots:
        for folder, dirs, names in os.walk(root):
            dirs[:] = [d for d in dirs if d not in (".git", "node_modules", "Library", ".Trash")]
            if ".DS_Store" in names:
                yield os.path.join(folder, ".DS_Store")


def main():
    binary, roots = sys.argv[1], sys.argv[2:]
    stats, codes, problems = Counter(), Counter(), Counter()
    for path in files(roots):
        stats["files"] += 1
        try:
            mine = dsxray_records(binary, path)
        except Exception as e:
            problems[f"dsxray failed: {type(e).__name__}"] += 1
            continue
        try:
            theirs = python_records(path)
        except Exception as e:
            problems[f"ds_store failed: {type(e).__name__}"] += 1
            stats["records only dsxray could read"] += len(mine)
            continue
        stats["records"] += len(theirs)
        codes.update(c for _, c, _, _ in theirs)
        if mine == theirs:
            stats["identical files"] += 1
        else:
            stats["files that differ"] += 1
            diff = [(a[1], a[2]) for a, b in zip(mine, theirs) if a != b]
            problems.update(f"differs on {c}/{t}" for c, t in diff)
            if len(mine) != len(theirs):
                problems["different record count"] += 1
    print(json.dumps({"stats": stats, "problems": problems, "record codes seen": dict(codes.most_common())}, indent=1))


if __name__ == "__main__":
    main()
