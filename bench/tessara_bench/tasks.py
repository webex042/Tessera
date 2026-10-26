"""Builds prompts from crates/tsr-infer/prompts and runs each eval (plan §16, §18.3)."""
from __future__ import annotations

import json
import random
from datetime import datetime
from pathlib import Path

from . import scoring
from .schema import validate

ROOT = Path(__file__).resolve().parents[2]
PROMPTS = ROOT / "crates" / "tsr-infer" / "prompts"
EVALS = ROOT / "evals"

USER_NAME = "Asha Verma"
USER_ALIASES = "Asha, AV, asha.verma@northwind.example, asha-v"

# Output budgets from plan §16.
MAX_TOKENS = {"extraction": 600, "ask": 450, "draft": 700}
TEMPERATURE = {"extraction": 0.0, "ask": 0.2, "draft": 0.6}


def load_prompt(kind: str, version: str = "v1") -> tuple[str, str, dict | None]:
    d = PROMPTS / kind / version
    schema_path = d / "schema.json"
    schema = json.loads(schema_path.read_text(encoding="utf-8")) if schema_path.exists() else None
    return (d / "system.txt").read_text(encoding="utf-8"), (d / "user.txt").read_text(encoding="utf-8"), schema


def fill(template: str, **values: str) -> str:
    # str.format would choke on JSON braces inside content; replace named slots only.
    out = template
    for k, v in values.items():
        out = out.replace("{" + k + "}", v)
    return out.replace("{{", "{").replace("}}", "}")


def escape_content(text: str) -> str:
    """§6.4 step 1: escape our own delimiter tags inside untrusted content."""
    for tag in ("screen_content", "memory", "thread", "style_examples"):
        text = text.replace(f"<{tag}>", f"&lt;{tag}&gt;").replace(f"</{tag}>", f"&lt;/{tag}&gt;")
    return text


def load_items() -> dict[str, dict]:
    items = {}
    for f in sorted((EVALS / "extraction" / "items").glob("*.json")):
        for it in json.loads(f.read_text(encoding="utf-8")):
            items[it["id"]] = it
    return items


def parse_json(content: str, schema: dict | None) -> tuple[dict | None, bool, list[str]]:
    try:
        obj = json.loads(content)
    except json.JSONDecodeError as e:
        return None, False, [f"json: {e}"]
    errs = validate(obj, schema) if schema else []
    return (obj if not errs else None), not errs, errs


def extraction_messages(it: dict) -> list[dict]:
    system, user, _ = load_prompt("extraction")
    ts = datetime.fromisoformat(it["captured_at"])
    return [
        {"role": "system", "content": fill(system, user_name=USER_NAME, user_aliases=USER_ALIASES)},
        {"role": "user", "content": fill(user, app_name=it["app"], title=it["title"], url_or_path=it["source"],
                                         local_datetime=ts.strftime("%Y-%m-%d %H:%M"), weekday=ts.strftime("%A"),
                                         tz="Asia/Kolkata (UTC+05:30)", redacted_text=escape_content(it["text"]))},
    ]


def snippet_line(n: int, it: dict) -> str:
    ts = datetime.fromisoformat(it["captured_at"])
    body = escape_content(it["text"]).replace("\n", " / ")
    return f"[{n}] {ts.strftime('%Y-%m-%d %a %H:%M')} · {it['app']} · {it['title']} — \"{body}\""


def ask_snippets(q: dict, items: dict[str, dict], k: int = 8, seed: int = 7) -> list[dict]:
    """Stage 0 isolates answer quality from retrieval: gold items plus seeded distractors.
    Retrieval quality is measured separately (bench/embed-eval, recall@k)."""
    rng = random.Random(f"{seed}-{q['id']}")
    gold = [items[i] for i in q["gold_item_ids"]]
    others = [it for iid, it in sorted(items.items()) if iid not in q["gold_item_ids"]]
    chosen = gold + rng.sample(others, k - len(gold))
    rng.shuffle(chosen)
    return chosen


def ask_messages(q: dict, snippets: list[dict]) -> list[dict]:
    system, user, _ = load_prompt("ask")
    ts = datetime.fromisoformat(q["asked_at"])
    lines = "\n".join(snippet_line(n + 1, it) for n, it in enumerate(snippets))
    return [
        {"role": "system", "content": system},
        {"role": "user", "content": fill(user, local_date=ts.strftime("%Y-%m-%d"), weekday=ts.strftime("%A"),
                                         snippets=lines, question=q["question"])},
    ]


def draft_messages(sc: dict, items: dict[str, dict]) -> tuple[list[dict], str]:
    system, user, _ = load_prompt("draft")
    thread = items[sc["thread_item_id"]]["text"]
    mem = [items[i] for i in sc["memory_item_ids"]]
    lines = "\n".join(snippet_line(n + 1, it) for n, it in enumerate(mem)) or "(none)"
    msgs = [
        {"role": "system", "content": fill(system, app_name=sc["app"])},
        {"role": "user", "content": fill(user, style_examples="(none yet)", thread=escape_content(thread),
                                         snippets=lines, instruction=sc["instruction"])},
    ]
    context = "\n".join([thread, sc["instruction"]] + [it["text"] for it in mem])
    return msgs, context
