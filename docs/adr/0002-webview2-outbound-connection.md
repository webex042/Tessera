# ADR-0002: WebView2 opens an outbound HTTPS connection we have not switched off

- Status: **Proposed — needs a decision by the project owner**
- Affects: S1 (§1.3), D1, §13.5, §18.6, Stage 0 WebView2 network spike, Stage 7

## Context

With every mitigation from §13.5 in place (wry defaults kept, SmartScreen off and read back,
background-networking switches, `--no-proxy-server`, navigation lock, `WEBVIEW2_*` variables
cleared), the WebView2 browser process started by Tessara still opened one TCP connection to port
443 in Microsoft's `2603:1046:c04::/48` range about a minute after start, in each of three runs
(4–10 minutes each, runtime 155.0.4283.62). The Chromium NetLog of the same run has no record of it,
and its host name was not in the Windows DNS cache. It therefore comes from code in
`msedgewebview2.exe` outside Chromium's network service; the most likely candidate is the
Edge/WebView2 diagnostic-data client, but this is **not confirmed**. Tessara's own process made no
connections. Evidence: `docs/stage0/network-test.md`.

S1 says "zero connections from Tessara processes to anything other than 127.0.0.1". As measured,
S1 fails because of the WebView2 child process.

## Options

1. **Identify, then decide.** Run the two-hour test with an elevated packet capture to read the
   TLS server name. If it is a documented, switchable endpoint (policy, setting or feature flag),
   switch it off and re-test. Cheapest next step; no design change yet.
2. **Native notch (plan D1 fallback).** Draw the always-on pill/peek/expanded notch natively
   (`windows-rs` + Direct2D/DirectComposition) and use WebView2 only for windows the user opens
   on purpose (settings, onboarding), which are closed afterwards. Removes the always-on WebView2
   process; large UI cost; S1 would still need a carve-out while settings are open.
3. **Narrow S1.** Re-word S1 to cover processes whose network behaviour Tessara controls and
   document WebView2's connection in `privacy.md`. Honest but weakens the headline promise
   ("nothing leaves your PC") — the README would have to change too.
4. **Block at the OS firewall.** Ship an optional outbound-block firewall rule for Tessara's
   WebView2 user-data folder / process. Needs admin at install, is easy to undo by mistake, and
   blocks rather than explains.

## Recommendation

Option 1 first (it decides between the others), with option 2 as the planned fallback. Do not
re-word S1 (option 3) unless option 1 shows the endpoint cannot be switched off and option 2 is
rejected for cost.

## Decision

Pending. Until decided, S1 is recorded as failing and Stage 0 cannot close.
