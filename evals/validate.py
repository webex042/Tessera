"""Validate the synthetic eval sets (plan §17 Stage 0 task 3, §18.3).

Checks: JSON parses, ids unique, splits/kinds valid, every gold `evidence` and `due_text`
is a verbatim substring of the item text (the same rule the extraction prompt imposes on the
model, §16.1 rule 3 and 6), and cross-references between datasets resolve.

    python evals/validate.py
"""
from __future__ import annotations

import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent
KINDS = {"task", "commitment", "decision", "event", "question", "note"}
SPLITS = {"dev", "heldout"}


def load_extraction() -> list[dict]:
    items = []
    for f in sorted((ROOT / "extraction" / "items").glob("*.json")):
        items += json.loads(f.read_text(encoding="utf-8"))
    return items


def main() -> int:
    errors: list[str] = []
    items = load_extraction()
    ids = Counter(i["id"] for i in items)
    errors += [f"duplicate id {k}" for k, n in ids.items() if n > 1]
    for it in items:
        iid, text = it["id"], it["text"]
        if it.get("split") not in SPLITS:
            errors.append(f"{iid}: bad split {it.get('split')}")
        for k in ("app", "title", "source", "captured_at", "text", "expected"):
            if k not in it:
                errors.append(f"{iid}: missing {k}")
        for n, f in enumerate(it["expected"]["facts"]):
            if f["kind"] not in KINDS:
                errors.append(f"{iid}#{n}: bad kind {f['kind']}")
            if f["evidence"] not in text:
                errors.append(f"{iid}#{n}: evidence not verbatim: {f['evidence']!r}")
            if len(f["evidence"]) > 200:
                errors.append(f"{iid}#{n}: evidence > 200 chars")
            if f.get("due_text") and f["due_text"] not in text:
                errors.append(f"{iid}#{n}: due_text not verbatim: {f['due_text']!r}")
        for s in it["expected"].get("forbidden_substrings", []):
            if s not in text:
                errors.append(f"{iid}: forbidden substring {s!r} not in text (pointless check)")

    item_ids = set(ids)
    extra = {}
    for name in ("ask", "draft"):
        f = ROOT / name / f"{name}.json"
        if f.exists():
            data = json.loads(f.read_text(encoding="utf-8"))
            extra[name] = data
            for q in data:
                refs = q.get("gold_item_ids", []) + q.get("memory_item_ids", []) + ([q["thread_item_id"]] if "thread_item_id" in q else [])
                for ref in refs:
                    if ref not in item_ids:
                        errors.append(f"{name} {q['id']}: unknown item {ref}")

    facts = Counter(f["kind"] for it in items for f in it["expected"]["facts"])
    splits = Counter(it["split"] for it in items)
    print(f"extraction: {len(items)} items ({dict(splits)}), {sum(facts.values())} gold facts {dict(facts)}")
    print(f"  items with no facts: {sum(1 for it in items if not it['expected']['facts'])}; injection items: {sum(1 for it in items if 'injection' in it['tags'])}")
    for name, data in extra.items():
        print(f"{name}: {len(data)} entries")
    if errors:
        print(f"\n{len(errors)} problem(s):")
        for e in errors:
            print("  " + e)
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
