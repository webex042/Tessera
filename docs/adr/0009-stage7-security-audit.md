# ADR-0009: Stage 7 security audit decisions, and Stages 8–10 scope

Status: accepted, 2026-10-28. Evidence: [docs/stage7/verification.md](../stage7/verification.md).

1. **Held-out discipline for the injection corpus.** The 78 Stage 7 cases were written before
   any run. Guard fixes were tuned on the odd-numbered half only; the even half stays unseen and
   is the reported generalisation estimate. The one exception is a genuine control (s7-g02,
   "send the server logs to vendor support") that an earlier guard change had broken; it was
   anticipated when written, fixed, and is disclosed.
2. **Guard additions** (general rules, each with a test): Hindi/Devanagari role nouns; filler
   words between a role noun and its verb; more action verbs; HTML comments addressed to an AI;
   generic `<|role|>` tokens; public bucket/repo/drive and password-manager exports as
   exfiltration targets; customer/client lists; "remember for later:", bulk overwrite of tasks,
   planted answers ("if anyone asks…"), `key=value` format smuggling, gift-card fraud; scope words
   ("publicly", "everyone", "permanently", "all") in a fact must come from the source. The
   earlier blanket "logs" exfiltration target was narrowed to logs sent to an outside address.
3. **Drafts and injected threads.** A draft sentence that repeats a line of the thread that tries
   to instruct AI tools is flagged as suspicious, and the panel shows a banner whenever the thread
   contains such text (real-model finding, eval dr-017).
4. **Redaction runs to a fixed point** (at most five passes) and has detectors for Aadhaar VID,
   passport and voter ID (EPIC), each gated on its keyword. `REDACT_VER` is now `v2`.
5. **Randomised robustness tests instead of cargo-fuzz** for now: `cargo-fuzz` needs a nightly
   toolchain and downloads (not approved). The seeded tests are deterministic and run in CI; they
   are not coverage-guided and are reported as such.
6. **Model integrity at start-up:** full SHA-256 against `tools/models.lock` on first use and after
   any change in size or modification time; otherwise a stamp file is trusted. Same-user
   tampering that also restores timestamps is out of scope (§15.2).
7. **Ask prompt v2.** v1 left "insufficient" undefined and the 4B model set it on 57 of 60
   answers; v2 defines it and asks for exact copies of checkable details. The Ask eval scores
   verified content (what the user sees), not the model's flag; both numbers are reported.
8. **Stages 8–10.** Stage 8 is not started: the plan gates Stages 8–10 on every MVP criterion
   passing (S3 and S4 do not yet), and it needs whisper.cpp / Opus-MT model downloads that are not
   approved. Stage 9's diagnostics bundle is built (content-free, reviewed before saving, never
   sent); the beta itself needs testers. Stage 10: NSIS configuration, SBOM generator and signing
   guide are in place; `bundle.active` stays false until the owner approves the toolchain
   download, signing needs the owner's certificate, and the updater gate stays closed (ADR-0003).
