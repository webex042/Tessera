"""Fail if any workspace crate other than tsr-net can reach network-capable code.

Plan D10 / section 20.3, Stage 0 "dependency spike". cargo-deny's `wrappers` only checks the
*direct* edge to a banned crate; this script walks the full resolved graph from `cargo metadata`
(normal and build dependencies, not dev) so transitive paths are caught too. Build dependencies
matter: a build script that downloads (e.g. ort-sys `download-binaries`) runs on every build.

Usage:
    python tools/check_network_deps.py                      # default build, no updater
    python tools/check_network_deps.py --features updater   # allows tauri-plugin-updater
    python tools/check_network_deps.py --manifest-path some/Cargo.toml   # self-test fixtures

Exit code 0 = clean, 1 = violation found, 2 = could not run cargo.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import deque

# Crates whose presence means "can open network connections or speak TLS/HTTP as a client".
# Matching is by crate name. Keep the list in sync with deny.toml [bans].
NETWORK_CRATES = {
    "reqwest", "ureq", "curl", "curl-sys", "isahc", "attohttpc", "surf",
    "hyper-tls", "hyper-rustls", "native-tls", "rustls", "tokio-rustls", "tokio-native-tls",
    "hf-hub", "tungstenite", "tokio-tungstenite", "async-tungstenite",
}

# Workspace members allowed to reach network crates, and why.
ALLOWED_MEMBERS = {"tsr-net"}
# Third-party crates allowed to pull network code only when a named feature is enabled.
FEATURE_GATED_ALLOW = {"updater": {"tauri-plugin-updater"}}


def cargo_metadata(manifest_path: str | None, features: list[str], locked: bool) -> dict:
    cmd = ["cargo", "metadata", "--format-version", "1",
           "--filter-platform", "x86_64-pc-windows-msvc"]
    if locked:
        cmd.append("--locked")
    if manifest_path:
        cmd += ["--manifest-path", manifest_path]
    if features:
        cmd += ["--features", ",".join(features)]
    try:
        out = subprocess.run(cmd, check=True, capture_output=True, text=True, encoding="utf-8")
    except FileNotFoundError:
        print("cargo not found on PATH", file=sys.stderr)
        sys.exit(2)
    except subprocess.CalledProcessError as e:
        print(e.stderr, file=sys.stderr)
        sys.exit(2)
    return json.loads(out.stdout)


def find_violations(meta: dict, allowed_third_party: set[str]) -> list[list[str]]:
    names = {p["id"]: p["name"] for p in meta["packages"]}
    edges: dict[str, list[str]] = {}
    for node in meta["resolve"]["nodes"]:
        targets = []
        for dep in node["deps"]:
            kinds = {k["kind"] for k in dep["dep_kinds"]}
            # kind None = normal, "build" = build script; skip "dev" (tests only).
            if kinds - {"dev"}:
                targets.append(dep["pkg"])
        edges[node["id"]] = targets

    violations = []
    for member in meta["workspace_members"]:
        if names[member] in ALLOWED_MEMBERS:
            continue
        # BFS keeping one shortest path per reached crate so the report shows the chain.
        parent = {member: None}
        queue = deque([member])
        while queue:
            cur = queue.popleft()
            for nxt in edges.get(cur, []):
                if nxt in parent:
                    continue
                # Do not descend through an explicitly allowed third-party wrapper.
                if names[nxt] in allowed_third_party:
                    continue
                parent[nxt] = cur
                if names[nxt] in NETWORK_CRATES:
                    chain, at = [], nxt
                    while at is not None:
                        chain.append(names[at])
                        at = parent[at]
                    violations.append(list(reversed(chain)))
                    continue
                queue.append(nxt)
    return violations


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest-path")
    ap.add_argument("--features", default="", help="comma-separated workspace features")
    ap.add_argument("--locked", action="store_true", help="require Cargo.lock to be up to date (CI)")
    args = ap.parse_args()
    features = [f for f in args.features.split(",") if f]

    allowed = set()
    for f in features:
        allowed |= FEATURE_GATED_ALLOW.get(f.split("/")[-1], set())

    meta = cargo_metadata(args.manifest_path, features, args.locked)
    violations = find_violations(meta, allowed)
    label = f"features={features or 'default'}"
    if violations:
        print(f"FAIL ({label}): network-capable crates reachable outside tsr-net:")
        for chain in violations:
            print("   " + " -> ".join(chain))
        return 1
    members = sorted(p["name"] for p in meta["packages"] if p["id"] in meta["workspace_members"])
    print(f"PASS ({label}): no network-capable crates reachable from {', '.join(members)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
