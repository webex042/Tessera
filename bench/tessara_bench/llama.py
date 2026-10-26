"""llama-server lifecycle and loopback client for benchmarks (plan §7.1, §7.3).

The command line here is the one Stage 0 verified against llama.cpp b11900
(docs/stage0/llama-server-b11900-help.txt). The server is started with a *cleared*
environment: llama-server reads LLAMA_ARG_* variables that can switch on agent tools,
MCP servers or Hugging Face downloads, so nothing is inherited from the caller.
"""
from __future__ import annotations

import json
import os
import secrets
import shutil
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

# Environment variables a Windows process needs to start; everything else is dropped.
_ENV_KEEP = ("SystemRoot", "SystemDrive", "windir", "TEMP", "TMP", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE")


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def server_args(model: Path, mmproj: Path | None, port: int, api_key: str, *, ctx: int = 8192, ngl: int = 99,
                kv_type: str = "q8_0", threads: int | None = None, flash_attn: str = "on") -> list[str]:
    """Plan §7.1 command line, corrected for b11900 (see docs/stage0/findings.md)."""
    args = [
        "--host", "127.0.0.1", "--port", str(port), "--api-key", api_key,
        "-m", str(model),
        "-ngl", str(ngl), "-c", str(ctx), "-np", "1",
        "-fa", flash_attn, "-ctk", kv_type, "-ctv", kv_type,
        "--jinja",
        # Network/attack-surface hardening verified present in b11900 --help:
        "--no-webui", "--offline", "--no-agent", "--no-ui-mcp-proxy", "--no-slots",
        "--no-cors-credentials", "--cors-origins", "http://tessara.invalid",
        # Determinism and memory: fail loudly rather than silently shrink; bound RAM prompt cache.
        "--fit", "off", "--cache-ram", "0",
        "--reasoning", "off",
    ]
    if mmproj:
        args += ["--mmproj", str(mmproj)]
    if threads:
        args += ["-t", str(threads)]
    return args


class VramSampler:
    """Samples GPU memory with nvidia-smi at ~10 Hz (plan §17 Stage 0 task 2)."""

    def __init__(self) -> None:
        self.exe = shutil.which("nvidia-smi")
        self.peak_mib: int | None = None
        self.baseline_mib: int | None = None
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    @property
    def available(self) -> bool:
        return self.exe is not None

    def _read(self) -> int | None:
        try:
            out = subprocess.run([self.exe, "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
                                 capture_output=True, text=True, timeout=5).stdout
            return max(int(x) for x in out.split())
        except Exception:
            return None

    def start(self) -> None:
        if not self.available:
            return
        self.baseline_mib = self._read()

        def loop() -> None:
            while not self._stop.is_set():
                v = self._read()
                if v is not None:
                    self.peak_mib = max(self.peak_mib or 0, v)
                self._stop.wait(0.1)

        self._thread = threading.Thread(target=loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(timeout=2)


@dataclass
class Server:
    base_url: str
    api_key: str
    proc: subprocess.Popen | None = None
    cold_load_s: float | None = None
    log_path: Path | None = None

    @classmethod
    def launch(cls, exe: Path, model: Path, mmproj: Path | None, log_dir: Path, **kw) -> "Server":
        port = free_port()
        key = secrets.token_hex(32)
        env = {k: os.environ[k] for k in _ENV_KEEP if k in os.environ}
        log_dir.mkdir(parents=True, exist_ok=True)
        log_path = log_dir / "llama-server.log"
        log = open(log_path, "w", encoding="utf-8", errors="replace")
        flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        t0 = time.perf_counter()
        proc = subprocess.Popen([str(exe), *server_args(model, mmproj, port, key, **kw)], env=env,
                                stdout=log, stderr=subprocess.STDOUT, creationflags=flags)
        srv = cls(f"http://127.0.0.1:{port}", key, proc, log_path=log_path)
        srv.wait_healthy(timeout_s=600)
        srv.cold_load_s = time.perf_counter() - t0
        return srv

    def _req(self, path: str, body: dict | None = None, stream: bool = False, timeout: float = 600):
        if not self.base_url.startswith("http://127.0.0.1:"):
            raise ValueError("loopback only (plan §7.3)")
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(self.base_url + path, data=data, method="POST" if body is not None else "GET",
                                     headers={"Content-Type": "application/json", "Authorization": f"Bearer {self.api_key}"})
        # An empty ProxyHandler ignores HTTP(S)_PROXY variables (§7.3).
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        return opener.open(req, timeout=timeout)

    def wait_healthy(self, timeout_s: float) -> None:
        deadline = time.time() + timeout_s
        while time.time() < deadline:
            if self.proc and self.proc.poll() is not None:
                raise RuntimeError(f"llama-server exited with {self.proc.returncode}; see {self.log_path}")
            try:
                with self._req("/health", timeout=5) as r:
                    if r.status == 200:
                        return
            except (urllib.error.URLError, ConnectionError, TimeoutError):
                pass
            time.sleep(0.25)
        raise TimeoutError("llama-server did not become healthy")

    def chat(self, messages: list[dict], schema: dict | None, *, max_tokens: int, temperature: float,
             stream: bool) -> "ChatResult":
        body = {"messages": messages, "max_tokens": max_tokens, "temperature": temperature, "stream": stream,
                "cache_prompt": True}
        if schema is not None:
            body["response_format"] = {"type": "json_schema", "json_schema": {"name": "out", "schema": schema, "strict": True}}
        t0 = time.perf_counter()
        res = ChatResult()
        if not stream:
            with self._req("/v1/chat/completions", body) as r:
                payload = json.loads(r.read())
            res.total_s = time.perf_counter() - t0
            res.content = payload["choices"][0]["message"].get("content") or ""
            res.timings = payload.get("timings", {})
            return res
        with self._req("/v1/chat/completions", body) as r:
            parts = []
            for raw in r:
                line = raw.decode("utf-8", "replace").strip()
                if not line.startswith("data:"):
                    continue
                data = line[5:].strip()
                if data == "[DONE]":
                    break
                chunk = json.loads(data)
                if "timings" in chunk:
                    res.timings = chunk["timings"]
                for ch in chunk.get("choices", []):
                    delta = ch.get("delta", {}).get("content")
                    if delta:
                        if res.ttft_s is None:
                            res.ttft_s = time.perf_counter() - t0
                        parts.append(delta)
            res.content = "".join(parts)
        res.total_s = time.perf_counter() - t0
        return res

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.proc.kill()


@dataclass
class ChatResult:
    content: str = ""
    ttft_s: float | None = None
    total_s: float = 0.0
    timings: dict = field(default_factory=dict)

    @property
    def prompt_tps(self) -> float | None:
        return self.timings.get("prompt_per_second")

    @property
    def gen_tps(self) -> float | None:
        return self.timings.get("predicted_per_second")
