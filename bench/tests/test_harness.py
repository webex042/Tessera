"""Harness self-tests. SIMULATED: they use bench/fake_server.py, not a model.

    python -m unittest discover -s bench/tests -v
"""
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import fake_server  # noqa: E402
from tessara_bench import scoring, tasks  # noqa: E402
from tessara_bench.llama import Server, server_args  # noqa: E402
from tessara_bench.schema import validate  # noqa: E402


class SchemaTests(unittest.TestCase):
    def setUp(self):
        _, _, self.schema = tasks.load_prompt("extraction")

    def test_valid(self):
        ok = {"summary": "x", "facts": [{"kind": "task", "text": "t", "owner": "me", "evidence": "e", "confidence": "low"}], "people": []}
        self.assertEqual(validate(ok, self.schema), [])

    def test_rejects_unknown_kind_extra_key_and_length(self):
        bad = {"summary": "x" * 281, "facts": [{"kind": "wish", "text": "t", "owner": "me", "evidence": "e", "confidence": "low", "x": 1}], "people": []}
        errs = validate(bad, self.schema)
        self.assertTrue(any("enum" in e for e in errs))
        self.assertTrue(any("unexpected key x" in e for e in errs))
        self.assertTrue(any("longer than 280" in e for e in errs))


class ScoringTests(unittest.TestCase):
    def test_evidence_normalisation_and_fuzzy(self):
        text = "Sure Ravi, I’ll send the revised sheet by Friday."
        self.assertTrue(scoring.evidence_found("I'll send the revised sheet by Friday", text))
        self.assertTrue(scoring.evidence_found("i'll  SEND the revised sheet by friday", text))
        self.assertFalse(scoring.evidence_found("I'll send the budget by Monday", text))

    def test_unsupported_claims(self):
        self.assertEqual(scoring.unsupported_claims("Sure, by Friday at 5pm.", "send it Friday"), ["5pm"])
        self.assertEqual(scoring.unsupported_claims("Paid 1500 on Monday", "pay 1500 by Friday"), ["Monday"])
        self.assertEqual(scoring.unsupported_claims("See you at 11 am", "call at 11 am"), [])

    def test_sentence_verification(self):
        snip = "BigRetail QBR is on Wednesday, 4 November 2026 14:00"
        self.assertTrue(scoring.sentence_verified("The BigRetail QBR is on 4 November at 14:00.", [snip]))
        self.assertFalse(scoring.sentence_verified("The QBR is on 5 November.", [snip]))


class PromptTests(unittest.TestCase):
    def test_delimiters_in_content_are_escaped(self):
        it = tasks.load_items()["inj-003"]
        user = tasks.extraction_messages(it)[1]["content"]
        # Exactly one real opening and one real closing tag: the ones we add.
        self.assertEqual(user.count("<screen_content>"), 1)
        self.assertEqual(user.count("</screen_content>"), 1)

    def test_server_args_are_hardened(self):
        a = server_args(Path("m.gguf"), None, 1234, "k")
        for flag in ("--no-webui", "--offline", "--no-agent", "--no-ui-mcp-proxy", "--cache-ram"):
            self.assertIn(flag, a)
        self.assertEqual(a[a.index("--host") + 1], "127.0.0.1")
        self.assertEqual(a[a.index("-c") + 1], "8192")

    def test_client_refuses_non_loopback(self):
        with self.assertRaises(ValueError):
            Server("http://192.168.1.5:8080", "k").wait_healthy(1)


class EndToEndSimulated(unittest.TestCase):
    """Pipeline + scorer against the fake server: perfect answers must score perfectly."""

    def _run(self, noise: bool) -> scoring.ExtractionScore:
        srv = fake_server.serve(0, noise)
        try:
            client = Server(f"http://127.0.0.1:{srv.server_address[1]}", fake_server.API_KEY)
            _, _, schema = tasks.load_prompt("extraction")
            sc = scoring.ExtractionScore()
            for it in tasks.load_items().values():
                r = client.chat(tasks.extraction_messages(it), schema, max_tokens=600, temperature=0, stream=False)
                obj, valid, _ = tasks.parse_json(r.content, schema)
                scoring.score_extraction_item(sc, it, obj, valid)
            return sc
        finally:
            srv.shutdown()

    def test_gold_answers_score_perfectly(self):
        sc = self._run(noise=False)
        self.assertEqual(sc.items, 80)
        self.assertEqual(sc.json_validity, 1.0)
        self.assertEqual(sc.action.f1, 1.0)
        self.assertEqual(sc.hallucinated, 0)
        self.assertEqual(sc.injection_failures, 0)

    def test_scorer_catches_injected_noise(self):
        sc = self._run(noise=True)
        self.assertGreater(sc.hallucinated, 0)
        self.assertLess(sc.action.precision, 1.0)

    def test_streaming_ttft_is_measured(self):
        srv = fake_server.serve(0, False)
        try:
            client = Server(f"http://127.0.0.1:{srv.server_address[1]}", fake_server.API_KEY)
            q = json.loads((tasks.EVALS / "ask" / "ask.json").read_text(encoding="utf-8"))[0]
            r = client.chat(tasks.ask_messages(q, tasks.ask_snippets(q, tasks.load_items())), None, max_tokens=50, temperature=0, stream=True)
            self.assertIsNotNone(r.ttft_s)
            self.assertEqual(r.gen_tps, 100.0)
        finally:
            srv.shutdown()


if __name__ == "__main__":
    unittest.main()
