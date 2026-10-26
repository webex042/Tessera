# ADR-0003: Optional updater stays out of builds until its zip dependency is fixed

- Status: Accepted (2026-10-26)
- Affects: D10, §20.3, Stage 10

## Context

The optional updater (`tauri-plugin-updater` 2.14.0, `--features updater`) is the single documented
exception to "network code only in tsr-net". Stage 0 audited it: it is the only path for
`reqwest`/`rustls` into the app, as intended. It also depends on `zip` 4.6.1, which has three
advisories (RUSTSEC-2026-0367, -0368, -0369: symlinks escaping the extraction directory and
permission handling through symlinks). The fix is zip ≥ 9.0, outside the plugin's version range.

## Decision

- The `updater` feature stays off in every build, including release. No ignore entry is added for
  these advisories.
- CI runs `cargo deny --features tessara/updater check` on every push with `continue-on-error`, so
  the failure stays visible without blocking unrelated work.
- Stage 10's exit criteria require that job to pass (plugin upgraded, or a replacement updater
  path that verifies signatures and does not extract archives with symlinks).

## Consequences

No auto-update in v1 unless upstream ships a fix before Stage 10. Users update by installing a
signed new version manually.
