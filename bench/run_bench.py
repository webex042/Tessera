"""Stage 0 benchmark runner (plan §17 Stage 0 tasks 2-4, exit criteria).

Launch a pinned llama-server with a local model and run the evals:

    python bench/run_bench.py --server third_party/llama_cpp/b11900/cuda-12.4-x64/llama-server.exe \
        --model D:/models/Qwen3-VL-4B-Instruct-Q4_K_M.gguf \
        --mmproj D:/models/mmproj-Qwen3-VL-4B-Instruct-F16.gguf \
        --label q4km-ctx8k-q8kv-cuda

Or attach to a server that is already running (e.g. the fake server for self-tests):

    python bench/run_bench.py --url http://127.0.0.1:PORT --api-key KEY --label fake

Results go to bench/results/<date>-<label>/ (summary.md, per-item CSVs, drafts for blind rating).
Exit code 0 always; the go/no-go verdict is in summary.md and is decided by a human.
"""
from __future__ import annotations

import argparse
import csv
import json
import statistics
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from tessara_bench import scoring, tasks  # noqa: E402
from tessara_bench.llama import Server, VramSampler  # noqa: E402


def pct(values: list[float], q: float) -> float | None:
    if not values:
        return None
    s = sorted(values)
    return s[min(len(s) - 1, int(round(q * (len(s) - 1))))]


def fmt(v, nd: int = 2) -> str:
    return "n/a" if v is None else (f"{v:.{nd}f}" if isinstance(v, float) else str(v))


def write_csv(path: Path, rows: list[dict]) -> None:
    if not rows:
        return
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)


def run_extraction(srv: Server, items: dict, split: str, out: Path) -> dict:
    _, _, schema = tasks.load_prompt("extraction")
    sc = scoring.ExtractionScore()
    rows, lat, ptps, gtps = [], [], [], []
    for it in items.values():
        if split != "all" and it["split"] != split:
            continue
        r = srv.chat(tasks.extraction_messages(it), schema, max_tokens=tasks.MAX_TOKENS["extraction"],
                     temperature=tasks.TEMPERATURE["extraction"], stream=False)
        obj, valid, errs = tasks.parse_json(r.content, schema)
        rec = scoring.score_extraction_item(sc, it, obj, valid)
        rec.update(latency_s=round(r.total_s, 3), prompt_tokens=r.timings.get("prompt_n"), prompt_tps=r.prompt_tps,
                   gen_tps=r.gen_tps, errors="; ".join(errs)[:200])
        rows.append(rec)
        lat.append(r.total_s)
        if r.prompt_tps:
            ptps.append(r.prompt_tps)
        if r.gen_tps:
            gtps.append(r.gen_tps)
        print(f"  extract {it['id']}: valid={valid} pred={rec['pred']} gold={rec['gold']} {r.total_s:.1f}s", flush=True)
    write_csv(out / "extraction.csv", rows)
    return {
        "items": sc.items, "json_validity": sc.json_validity,
        "action_p": sc.action.precision, "action_r": sc.action.recall, "action_f1": sc.action.f1,
        "per_kind_f1": {k: round(v.f1, 3) for k, v in sorted(sc.per_kind.items())},
        "hallucination_rate": sc.hallucination_rate, "predicted_facts": sc.predicted_facts,
        "owner_accuracy": (sc.owner_correct / sc.owner_checked) if sc.owner_checked else None,
        "injection_failures": f"{sc.injection_failures}/{sc.injection_items}",
        "latency_p50_s": pct(lat, 0.5), "latency_p95_s": pct(lat, 0.95),
        "prompt_tps_median": statistics.median(ptps) if ptps else None,
        "gen_tps_median": statistics.median(gtps) if gtps else None,
    }


def run_ask(srv: Server, items: dict, split: str, out: Path) -> dict:
    _, _, schema = tasks.load_prompt("ask")
    qs = json.loads((tasks.EVALS / "ask" / "ask.json").read_text(encoding="utf-8"))
    rows, ttft, total, sent, ver, fab = [], [], [], 0, 0, 0
    for q in qs:
        if split != "all" and q["split"] != split:
            continue
        snips = tasks.ask_snippets(q, items)
        r = srv.chat(tasks.ask_messages(q, snips), schema, max_tokens=tasks.MAX_TOKENS["ask"],
                     temperature=tasks.TEMPERATURE["ask"], stream=True)
        obj, valid, _ = tasks.parse_json(r.content, schema)
        rec = scoring.score_ask(q, obj, valid, [s["text"] for s in snips])
        rec.update(ttft_s=r.ttft_s, total_s=round(r.total_s, 3), prompt_tokens=r.timings.get("prompt_n"))
        rows.append(rec)
        sent += rec["sentences"]
        ver += rec["verified"]
        fab += rec["fabricated_cites"]
        if r.ttft_s is not None:
            ttft.append(r.ttft_s)
        total.append(r.total_s)
        print(f"  ask {q['id']}: correct={rec['correct']} ttft={fmt(r.ttft_s)}s", flush=True)
    write_csv(out / "ask.csv", rows)
    n = len(rows)
    return {
        "questions": n, "accuracy": sum(r["correct"] for r in rows) / n if n else None,
        "verified_sentence_rate": ver / sent if sent else None, "fabricated_citations": fab,
        "ttft_p50_s": pct(ttft, 0.5), "total_p50_s": pct(total, 0.5),
    }


def run_draft(srv: Server, items: dict, split: str, out: Path) -> dict:
    _, _, schema = tasks.load_prompt("draft")
    scen = json.loads((tasks.EVALS / "draft" / "draft.json").read_text(encoding="utf-8"))
    rows, ttft, total = [], [], []
    blind = ["# Drafts for blind self-rating (1-5)\n", "Rate each draft without looking at the model label. Plan Stage 0 gate: average >= 3/5.\n"]
    for sc in scen:
        if split != "all" and sc["split"] != split:
            continue
        msgs, context = tasks.draft_messages(sc, items)
        r = srv.chat(msgs, schema, max_tokens=tasks.MAX_TOKENS["draft"], temperature=tasks.TEMPERATURE["draft"], stream=True)
        obj, valid, _ = tasks.parse_json(r.content, schema)
        rec = scoring.score_draft(sc, obj, valid, context)
        rec.update(ttft_s=r.ttft_s, total_s=round(r.total_s, 3), unsupported="|".join(rec["unsupported"]))
        rows.append(rec)
        if r.ttft_s is not None:
            ttft.append(r.ttft_s)
        total.append(r.total_s)
        blind.append(f"\n## {sc['id']}\n\nInstruction: {sc['instruction']}\n\n```\n{(obj or {}).get('draft', '(invalid output)')}\n```\n\nRating: _/5\n")
        print(f"  draft {sc['id']}: includes={rec['includes_ok']} excludes={rec['excludes_ok']}", flush=True)
    write_csv(out / "draft.csv", rows)
    (out / "drafts_for_rating.md").write_text("".join(blind), encoding="utf-8")
    n = len(rows)
    return {
        "scenarios": n,
        "auto_checks_pass_rate": sum(r["includes_ok"] and r["excludes_ok"] and r["placeholder_ok"] for r in rows) / n if n else None,
        "drafts_with_unflagged_unsupported_claims": "flags shown by code; count of drafts with any: "
                                                    + str(sum(1 for r in rows if r["unsupported"])),
        "ttft_p50_s": pct(ttft, 0.5), "total_p50_s": pct(total, 0.5),
    }


def gate(summary: dict) -> list[str]:
    """Stage 0 exit criteria (stages.md). Unmeasured items are reported as UNVERIFIED."""
    ex, ask, hw = summary.get("extraction", {}), summary.get("ask", {}), summary.get("hardware", {})
    lines = []

    def row(name, value, ok):
        status = "UNVERIFIED" if value is None else ("PASS" if ok else "FAIL")
        lines.append(f"| {name} | {fmt(value) if not isinstance(value, str) else value} | {status} |")

    peak = hw.get("vram_peak_mib")
    row("GPU memory peak (dedicated+shared, llama-server) <= 5.2 GB (5325 MiB)", peak, peak is not None and peak <= 5325)
    jv = ex.get("json_validity")
    row("Extraction JSON valid >= 99%", jv, jv is not None and jv >= 0.99)
    f1 = ex.get("action_f1")
    row("Task/commitment F1 >= 0.65", f1, f1 is not None and f1 >= 0.65)
    p50 = ex.get("latency_p50_s")
    row("Extraction p50 <= 8 s", p50, p50 is not None and p50 <= 8)
    t = ask.get("ttft_p50_s")
    row("Ask TTFT p50 <= 2.5 s", t, t is not None and t <= 2.5)
    lines.append("| Draft blind self-rating >= 3/5 | see drafts_for_rating.md | MANUAL |")
    lines.append("| UIA/OCR usable for >= 16/20 apps | see docs/stage0/app-compatibility.md | SEPARATE |")
    return lines


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--server", type=Path, help="llama-server.exe to launch")
    src.add_argument("--url", help="attach to a running server (loopback only)")
    ap.add_argument("--api-key", default="")
    ap.add_argument("--model", type=Path)
    ap.add_argument("--mmproj", type=Path)
    ap.add_argument("--ctx", type=int, default=8192)
    ap.add_argument("--kv-type", default="q8_0")
    ap.add_argument("--ngl", default="all", help="number, 'all' or 'auto' (from tsr-hw); 0 = CPU only")
    ap.add_argument("--device", help="backend device id from tsr-hw, e.g. CUDA0, Vulkan0, ROCm0, SYCL0, MTL0, or 'none'")
    ap.add_argument("--no-mmproj-offload", action="store_true", help="keep the vision encoder on the CPU")
    ap.add_argument("--label", required=True)
    ap.add_argument("--split", default="all", choices=["all", "dev", "heldout"])
    ap.add_argument("--only", default="extraction,ask,draft")
    args = ap.parse_args()

    out = Path(__file__).resolve().parent / "results" / f"{datetime.now():%Y%m%d-%H%M}-{args.label}"
    out.mkdir(parents=True, exist_ok=True)
    vram = VramSampler()
    vram.start()
    if args.server:
        if not args.model:
            ap.error("--model is required with --server")
        srv = Server.launch(args.server, args.model, args.mmproj, out, ctx=args.ctx, kv_type=args.kv_type, ngl=args.ngl,
                            device=args.device, mmproj_offload=not args.no_mmproj_offload)
    else:
        srv = Server(args.url, args.api_key)
        srv.wait_healthy(10)

    items = tasks.load_items()
    summary: dict = {"label": args.label, "split": args.split, "started": datetime.now().isoformat(timespec="seconds"),
                     "server": str(args.server or args.url), "model": str(args.model or "attached"),
                     "cold_load_s": srv.cold_load_s}
    try:
        only = set(args.only.split(","))
        if "extraction" in only:
            summary["extraction"] = run_extraction(srv, items, args.split, out)
        if "ask" in only:
            summary["ask"] = run_ask(srv, items, args.split, out)
        if "draft" in only:
            summary["draft"] = run_draft(srv, items, args.split, out)
    finally:
        vram.stop()
        if args.server:
            srv.stop()
    g = srv.gpu_mem if args.server else None
    peak = None if not (g and g.samples) else (g.peak_dedicated_mib or 0) + (g.peak_shared_mib or 0)
    summary["hardware"] = {
        "device": args.device, "ngl": args.ngl,
        # Vendor-neutral, per llama-server process (Windows WDDM counters).
        "gpu_mem_samples": g.samples if g else 0,
        "gpu_mem_peak_dedicated_mib": g.peak_dedicated_mib if g else None,
        "gpu_mem_peak_shared_mib": g.peak_shared_mib if g else None,
        "vram_peak_mib": peak,
        # Device-wide, NVIDIA only, when nvidia-smi exists.
        "nvidia_smi": vram.available, "nvidia_smi_peak_mib": vram.peak_mib,
    }

    (out / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    md = [f"# Stage 0 benchmark: {args.label}\n", f"- split: {args.split}\n- server: `{summary['server']}`\n- model: `{summary['model']}`\n",
          f"- cold load: {fmt(summary['cold_load_s'])} s\n", "\n## Gate\n\n| Criterion | Value | Status |\n|---|---|---|\n",
          "\n".join(gate(summary)) + "\n", "\n## Details\n\n```json\n" + json.dumps(summary, indent=2) + "\n```\n"]
    (out / "summary.md").write_text("".join(md), encoding="utf-8")
    print(f"\nwrote {out / 'summary.md'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
