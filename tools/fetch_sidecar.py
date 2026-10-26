"""Download, verify and unpack pinned external binaries listed in tools/sidecar.lock.

Plan §7.1 (llama-server builds per GPU backend) and §10.1 (ONNX Runtime). Developer/CI tool;
nothing here runs inside the app. Downloads happen only when this script is run by a person.

Integrity model:
- The archive must match the lock's SHA-256 (taken from the GitHub release-asset digest).
- After unpacking, EVERY regular file in the variant directory must be listed in the lock's
  `files` manifest with a matching SHA-256, and no unlisted file may exist. A llama.cpp build's
  executable is identical across backends; the GPU code lives in loadable `ggml-*.dll` files, so
  pinning only the executable would not pin the backend.
- `files: null` means "not recorded yet": the run fails unless `--record` is given, in which case
  the observed manifest is written back for review and commit.

    python tools/fetch_sidecar.py --list
    python tools/fetch_sidecar.py llama_cpp vulkan-x64 [--record]
    python tools/fetch_sidecar.py onnxruntime win-x64
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "tools" / "sidecar.lock"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_lock() -> dict:
    return json.loads(LOCK.read_text(encoding="utf-8"))


def save_lock(lock: dict) -> None:
    LOCK.write_text(json.dumps(lock, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")


def version_of(lock: dict, component: str) -> str:
    return lock[component].get("tag") or lock[component]["version"]


def find_asset(lock: dict, component: str, variant: str) -> dict:
    for a in lock[component]["assets"]:
        if a["variant"] == variant:
            return a
    sys.exit(f"no asset '{variant}' for '{component}' in sidecar.lock")


def variant_dir(lock: dict, component: str, asset: dict) -> Path:
    target = asset.get("extract_into", asset["variant"])
    return ROOT / "third_party" / component / version_of(lock, component) / target


def safe_extract(archive: Path, dest: Path) -> None:
    """Unpack without letting member paths escape `dest` (zip-slip / tar traversal)."""
    dest = dest.resolve()
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as z:
            for m in z.infolist():
                target = (dest / m.filename).resolve()
                if dest not in target.parents and target != dest:
                    sys.exit(f"refusing archive member outside destination: {m.filename}")
            z.extractall(dest)
    else:
        with tarfile.open(archive) as t:
            t.extractall(dest, filter="data")  # rejects absolute paths, '..', device files, links out


def clear_dir(dir_: Path) -> None:
    """Remove every file under `dir_`. Empty folders that cannot be removed are left: OneDrive
    and other sync clients hold handles on directories, and manifests only list files."""
    for p in sorted(dir_.rglob("*"), reverse=True):
        if p.is_file() or p.is_symlink():
            p.unlink()
        else:
            try:
                p.rmdir()
            except OSError:
                pass


def manifest(dir_: Path) -> dict[str, str]:
    return {p.relative_to(dir_).as_posix(): sha256(p) for p in sorted(dir_.rglob("*")) if p.is_file()}


def verify_dir(dir_: Path, expected: dict[str, str]) -> list[str]:
    """Problems found comparing a directory against its manifest (empty = OK)."""
    actual = manifest(dir_)
    problems = [f"missing {k}" for k in expected if k not in actual]
    problems += [f"hash mismatch {k}" for k, v in expected.items() if k in actual and actual[k] != v]
    problems += [f"unexpected file {k}" for k in actual if k not in expected]
    return problems


def fetch(component: str, variant: str, record: bool) -> int:
    lock = load_lock()
    asset = find_asset(lock, component, variant)
    cache = ROOT / "third_party" / ".cache"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / asset["name"]
    if not archive.exists() or sha256(archive) != asset["sha256"]:
        print(f"Downloading {asset['url']} ({asset['size'] / 1e6:.0f} MB)")
        opener = urllib.request.build_opener()
        with opener.open(asset["url"], timeout=60) as r, open(archive, "wb") as f:
            shutil.copyfileobj(r, f)
    got = sha256(archive)
    if got != asset["sha256"]:
        archive.unlink()
        sys.exit(f"archive hash mismatch for {asset['name']}: expected {asset['sha256']}, got {got}")
    print(f"Archive OK  {asset['name']}")

    dest = variant_dir(lock, component, asset)
    # Companion archives (e.g. CUDA runtime DLLs) unpack into another variant's directory;
    # that directory's manifest then covers both, so record/verify the combined result.
    if "extract_into" not in asset and dest.exists():
        clear_dir(dest)
    dest.mkdir(parents=True, exist_ok=True)
    safe_extract(archive, dest)

    owner = asset if "extract_into" not in asset else find_asset(lock, component, asset["extract_into"])
    if owner["files"] is None:
        if not record:
            sys.exit(f"no file manifest recorded for '{owner['variant']}'; rerun with --record on a trusted machine")
        owner["files"] = manifest(dest)
        save_lock(lock)
        print(f"Recorded {len(owner['files'])} files for '{owner['variant']}' in sidecar.lock - review and commit.")
    else:
        problems = verify_dir(dest, owner["files"])
        if problems:
            for p in problems[:20]:
                print("  " + p)
            sys.exit(f"{len(problems)} integrity problem(s) in {dest}")
        print(f"Files OK    {len(owner['files'])} files match the manifest")
    print(f"Unpacked to {dest}")
    return 0


def list_assets() -> int:
    lock = load_lock()
    for comp in ("llama_cpp", "onnxruntime"):
        for a in lock[comp]["assets"]:
            rec = "recorded" if a.get("files") else ("companion" if "extract_into" in a else "not recorded")
            print(f"{comp:12} {a['variant']:24} {a.get('os','windows'):8} {a.get('backend','-'):12} {a['size']/1e6:7.0f} MB  {rec}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("component", nargs="?", choices=["llama_cpp", "onnxruntime"])
    ap.add_argument("variant", nargs="?")
    ap.add_argument("--record", action="store_true")
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()
    if a.list:
        return list_assets()
    if not (a.component and a.variant):
        ap.error("component and variant are required (or use --list)")
    return fetch(a.component, a.variant, a.record)


if __name__ == "__main__":
    sys.exit(main())
