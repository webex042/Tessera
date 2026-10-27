"""CycloneDX 1.5 SBOM from local data only (Stage 10; no network, no extra tools).

    cargo deny --offline list -f json -l crate > deny-list.json
    (cd ui && pnpm licenses list --json --prod > pnpm-lic.json)
    python tools/gen_sbom.py deny-list.json pnpm-lic.json > sbom.cdx.json

Components: every Rust crate in the build graph (pkg:cargo purls) and every production UI package
(pkg:npm purls) with declared licences, plus the pinned binaries and model files with their
SHA-256 from tools/sidecar.lock and tools/models.lock. Release builds attach this file.
"""
import json
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path

root = Path(__file__).resolve().parent.parent
deny = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
pnpm = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
ours = {"tessara", "tsr-core", "tsr-win", "tsr-store", "tsr-retrieval", "tsr-guard", "tsr-infer", "tsr-capture",
        "tsr-net", "tsr-pipeline", "tsr-draft", "tsr-evals", "uia-probe"}


def lic(expr_parts):
    if not expr_parts:
        return []
    return [{"expression": " OR ".join(expr_parts)}]


components = []
for key, v in sorted(deny.items()):
    name, version = key.split(" ")[:2]
    if name in ours:
        continue
    components.append({"type": "library", "name": name, "version": version, "purl": f"pkg:cargo/{name}@{version}",
                       "licenses": lic(v.get("licenses", []))})
for license_expr, pkgs in sorted(pnpm.items()):
    for p in pkgs:
        for ver in p["versions"]:
            components.append({"type": "library", "name": p["name"], "version": ver,
                               "purl": f"pkg:npm/{p['name'].replace('@', '%40')}@{ver}",
                               "licenses": [{"expression": license_expr}]})

sidecar = json.loads((root / "tools" / "sidecar.lock").read_text(encoding="utf-8"))
for comp, spec in sidecar.items():
    if comp.startswith("_") or not isinstance(spec, dict):
        continue
    for a in spec.get("assets", []):
        if not a.get("sha256"):
            continue
        components.append({"type": "application", "name": f"{comp}/{a.get('variant', a['name'])}",
                           "version": spec.get("tag") or spec.get("version") or "",
                           "hashes": [{"alg": "SHA-256", "content": a["sha256"]}],
                           "externalReferences": [{"type": "distribution", "url": a.get("url", "")}]})
models = json.loads((root / "tools" / "models.lock").read_text(encoding="utf-8"))
for m in models.get("models", []):
    for f in m.get("files", []):
        components.append({"type": "machine-learning-model" if f["path"].endswith(".gguf") else "data",
                           "name": f"{m['id']}/{f['path']}", "version": m.get("revision", ""),
                           "licenses": [{"expression": m.get("license", "NOASSERTION")}],
                           # Small files are pinned by their git blob SHA-1 (tools/models.lock).
                           "hashes": [{"alg": "SHA-256", "content": f["sha256"]}] if "sha256" in f
                           else [{"alg": "SHA-1", "content": f["git_blob_sha1"]}],
                           "properties": [] if "sha256" in f else [{"name": "tessara:hash-kind", "value": "git-blob-sha1"}]})

app_version = "0.0.1"
for line in (root / "src-tauri" / "Cargo.toml").read_text(encoding="utf-8").splitlines():
    if line.startswith("version") and "workspace" not in line:
        app_version = line.split('"')[1]
sbom = {
    "bomFormat": "CycloneDX", "specVersion": "1.5", "serialNumber": f"urn:uuid:{uuid.uuid4()}", "version": 1,
    "metadata": {"timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                 "tools": [{"name": "tools/gen_sbom.py"}],
                 "component": {"type": "application", "name": "Tessara", "version": app_version}},
    "components": components,
}
sys.stdout.reconfigure(encoding="utf-8", newline="\n")
print(json.dumps(sbom, indent=1, ensure_ascii=False))
