"""SIMULATED OpenAI-compatible server for harness self-tests. Not a model.

It answers extraction prompts with the gold labels of the matching eval item (looked up by the
screen text), so a correct pipeline must score F1 = 1.0, 100% JSON validity and 0 hallucinations.
`--noise` corrupts answers in known ways to prove the scorer catches them. Ask and Draft prompts
get fixed, schema-valid replies. Results from this server say nothing about model quality.

    python bench/fake_server.py --port 0 [--noise]
"""
from __future__ import annotations

import argparse
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from tessara_bench import tasks  # noqa: E402

API_KEY = "fake-key"


def gold_reply(user_msg: str, items: dict, noise: bool) -> dict:
    for it in items.values():
        if tasks.escape_content(it["text"]) in user_msg:
            facts = []
            for f in it["expected"]["facts"]:
                facts.append({"kind": f["kind"], "text": f["text"], "owner": f["owner"], "counterparty": f["counterparty"],
                              "due_text": f["due_text"], "evidence": f["evidence"], "confidence": "high"})
            if noise and facts:
                facts[0] = dict(facts[0], evidence="this sentence was never on screen")
                facts.append({"kind": "task", "text": "invented task", "owner": "me", "counterparty": None,
                              "due_text": None, "evidence": "completely invented evidence", "confidence": "low"})
            return {"summary": it["title"][:280], "facts": facts, "people": []}
    return {"summary": "", "facts": [], "people": []}


def make_handler(items: dict, noise: bool):
    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):  # quiet
            pass

        def _auth(self) -> bool:
            if self.headers.get("Authorization") != f"Bearer {API_KEY}":
                self.send_response(401)
                self.end_headers()
                return False
            return True

        def do_GET(self):
            if self.path == "/health" and self._auth():
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b'{"status":"ok"}')

        def do_POST(self):
            if not self._auth():
                return
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            system = body["messages"][0]["content"]
            user = body["messages"][-1]["content"]
            if "structured memory" in system:
                reply = gold_reply(user, items, noise)
            elif "numbered memory snippets" in system:
                reply = {"sentences": [{"text": "See snippet 1.", "cites": [1]}], "insufficient": False}
            else:
                reply = {"draft": "Thanks, will do. [details]", "used": [], "placeholders": ["details"]}
            content = json.dumps(reply)
            timings = {"prompt_n": len(user) // 4, "prompt_per_second": 1000.0, "predicted_n": len(content) // 4, "predicted_per_second": 100.0}
            if body.get("stream"):
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                for i in range(0, len(content), 16):
                    chunk = {"choices": [{"delta": {"content": content[i:i + 16]}}]}
                    self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
                self.wfile.write(f"data: {json.dumps({'choices': [], 'timings': timings})}\n\n".encode())
                self.wfile.write(b"data: [DONE]\n\n")
            else:
                payload = {"choices": [{"message": {"content": content}}], "timings": timings}
                data = json.dumps(payload).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

    return H


def serve(port: int = 0, noise: bool = False) -> ThreadingHTTPServer:
    srv = ThreadingHTTPServer(("127.0.0.1", port), make_handler(tasks.load_items(), noise))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=0)
    ap.add_argument("--noise", action="store_true")
    a = ap.parse_args()
    s = serve(a.port, a.noise)
    print(f"SIMULATED server on http://127.0.0.1:{s.server_address[1]}  api-key={API_KEY}", flush=True)
    threading.Event().wait()
