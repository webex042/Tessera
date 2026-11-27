// AI Onboarding & Provider Setup (Stage 3): Local GGUF verification/download and Groq BYOK.
import { useEffect, useState } from "react";
import { api, on } from "../../ipc/api";
import type { AiSetupState, ConnectivityResult, DownloadStatus } from "../../ipc/types";
import { Status, useLoad } from "./common";

function fmtGiB(bytes: number): string {
  if (bytes >= 1 << 30) return `${(bytes / (1 << 30)).toFixed(2)} GiB`;
  return `${Math.round(bytes / (1 << 20))} MiB`;
}

export function SetupView({ onBack }: { onBack: () => void }) {
  const load = useLoad<AiSetupState>(() => api.aiSetup(), []);
  const data = load.data;

  const [groqKey, setGroqKey] = useState("");
  const [groqModel, setGroqModel] = useState("openai/gpt-oss-20b");
  const [consent, setConsent] = useState(false);
  const [downloadApproval, setDownloadApproval] = useState<"local_llm" | "embedder" | null>(null);
  const [dl, setDl] = useState<DownloadStatus | null>(null);
  const [probe, setProbe] = useState<ConnectivityResult | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (data) {
      setGroqModel(data.groq_model || "openai/gpt-oss-20b");
      setConsent(data.cloud_consent);
      setDl(data.download);
    }
  }, [data]);

  useEffect(() => {
    const p = on<DownloadStatus>("model_download_progress", (snap) => {
      setDl(snap);
      if (!snap.active) {
        load.reload();
      }
    });
    return () => {
      p.then((u) => u());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const switchLocal = () => {
    setBusy(true);
    setMsg(null);
    api.setAiProvider("local").then(
      () => {
        setBusy(false);
        setMsg("Switched to local inference mode.");
        load.reload();
      },
      () => {
        setBusy(false);
        setMsg("Could not switch to local mode.");
      },
    );
  };

  const saveGroq = () => {
    if (!consent) {
      setMsg("Please confirm the cloud PII disclosure before saving Groq settings.");
      return;
    }
    setBusy(true);
    setMsg(null);
    api.saveGroqConfig(groqKey.trim() ? groqKey.trim() : null, groqModel.trim(), consent).then(
      () => {
        setBusy(false);
        setGroqKey("");
        setMsg("Saved Groq configuration under Windows DPAPI and activated runtime.");
        load.reload();
      },
      (e) => {
        setBusy(false);
        setMsg(
          e.code === "cloud_consent_required"
            ? "Explicit cloud consent is required."
            : e.code === "feature_unavailable"
              ? "Groq feature is disabled in this build."
              : "Could not save Groq configuration.",
        );
      },
    );
  };

  const removeGroq = () => {
    setBusy(true);
    setMsg(null);
    setProbe(null);
    api.removeGroqConfig().then(
      () => {
        setBusy(false);
        setConsent(false);
        setGroqKey("");
        setMsg("Removed stored Groq API key and reverted to local mode.");
        load.reload();
      },
      () => {
        setBusy(false);
        setMsg("Could not remove Groq key.");
      },
    );
  };

  const testGroq = () => {
    setBusy(true);
    setProbe(null);
    setMsg(null);
    api.testGroqConnection(groqKey.trim() ? groqKey.trim() : null, groqModel.trim()).then(
      (res) => {
        setBusy(false);
        setProbe(res);
      },
      () => {
        setBusy(false);
        setProbe({ ok: false, latency_ms: 0, code: "unavailable" });
      },
    );
  };

  const beginDownload = (target: "local_llm" | "embedder") => {
    setBusy(true);
    setMsg(null);
    api.startModelDownload(target, true).then(
      (st) => {
        setBusy(false);
        setDownloadApproval(null);
        setDl(st.download);
      },
      (e) => {
        setBusy(false);
        setDownloadApproval(null);
        setMsg(
          e.code === "insufficient_disk_space"
            ? "Not enough free disk space in Tessara's data drive."
            : "Could not start verified download.",
        );
      },
    );
  };

  const activeDl = dl?.active ?? false;
  const pct = dl && dl.total_bytes > 0 ? Math.min(100, Math.round((dl.done_bytes / dl.total_bytes) * 100)) : 0;

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <button type="button" className="notch-button" onClick={onBack}>
          ← Back
        </button>
        <h3 className="font-semibold">AI Provider & Model Setup</h3>
        {data && (
          <span className="notch-chip ml-auto">
            {data.provider === "groq" ? "Cloud · Groq" : "Local · llama.cpp"} · {data.runtime_state.replace(/_/g, " ")}
          </span>
        )}
      </div>

      {msg && (
        <p role="status" className="rounded border border-[var(--notch-line)] px-2 py-1 text-[11px]">
          {msg}
        </p>
      )}

      <Status load={load} empty={false}>
        {data && (
          <>
            <section className="rounded-lg border border-[var(--notch-line)] p-2.5">
              <div className="flex items-center justify-between gap-2">
                <div>
                  <h4 className="text-[12px] font-semibold">Option A — Local Inference (Qwen3-VL-4B, offline)</h4>
                  <p className="notch-muted text-[11px]">
                    Hardware: {data.hardware.gpu_name ? `GPU (${data.hardware.gpu_name})` : `CPU-only (${data.hardware.cpu_cores} cores)`}
                    {data.hardware.ram_total_mib ? ` · ${(data.hardware.ram_total_mib / 1024).toFixed(0)} GiB RAM` : ""}
                    {data.hardware.disk_free_mib !== null ? ` · ${(data.hardware.disk_free_mib / 1024).toFixed(1)} GiB free` : ""}
                  </p>
                </div>
                <button
                  type="button"
                  className="notch-button whitespace-nowrap text-[11px]"
                  disabled={busy || data.provider === "local"}
                  onClick={switchLocal}
                >
                  {data.provider === "local" ? "Active" : "Use Local"}
                </button>
              </div>
              {data.hardware.warnings.map((w, i) => (
                <p key={i} className="notch-muted mt-1 text-[11px]">
                  Note: {w}
                </p>
              ))}
              <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px]">
                <span className="notch-chip">
                  LLM: {data.has_local_model ? "Verified on disk" : `Not downloaded (${fmtGiB(data.hardware.local_model_bytes)})`}
                </span>
                {!data.has_local_model && !activeDl && data.groq_feature_supported && (
                  <button type="button" className="notch-button" onClick={() => setDownloadApproval("local_llm")}>
                    Download Qwen3-VL
                  </button>
                )}
                <span className="notch-chip">
                  Vector Embedder: {data.has_embedder ? "Verified (E5-small ONNX)" : `Optional (${fmtGiB(data.hardware.embedder_bytes)})`}
                </span>
                {!data.has_embedder && !activeDl && data.groq_feature_supported && (
                  <button type="button" className="notch-button" onClick={() => setDownloadApproval("embedder")}>
                    Download Embedder
                  </button>
                )}
              </div>

              {downloadApproval && !activeDl && (
                <div className="mt-2 rounded border border-[var(--notch-line)] p-2 text-[11px]">
                  <p>
                    Download{" "}
                    {downloadApproval === "local_llm"
                      ? `Qwen3-VL-4B-Instruct (${fmtGiB(data.hardware.local_model_bytes)})`
                      : `multilingual-e5-small ONNX (${fmtGiB(data.hardware.embedder_bytes)})`}{" "}
                    over HTTPS from <code>huggingface.co</code> and verify SHA-256 against <code>tools/models.lock</code>?
                  </p>
                  <div className="mt-1.5 flex gap-2">
                    <button type="button" className="notch-button font-semibold" disabled={busy} onClick={() => beginDownload(downloadApproval)}>
                      Approve & download
                    </button>
                    <button type="button" className="notch-button" onClick={() => setDownloadApproval(null)}>
                      Cancel
                    </button>
                  </div>
                </div>
              )}

              {activeDl && dl && (
                <div className="mt-2 space-y-1 text-[11px]">
                  <div className="flex items-center justify-between">
                    <span>
                      {dl.phase}: {dl.file ?? dl.target} ({pct}%)
                    </span>
                    <button type="button" className="notch-button" onClick={() => api.cancelModelDownload().then(() => load.reload())}>
                      Cancel download
                    </button>
                  </div>
                  <div className="h-1.5 w-full overflow-hidden rounded bg-[var(--notch-hover)]">
                    <div className="h-full bg-[var(--notch-accent)]" style={{ width: `${pct}%` }} />
                  </div>
                </div>
              )}
              {dl?.error && !activeDl && <p className="mt-1 text-[11px]">Download issue: {dl.error}</p>}
            </section>

            <section className="rounded-lg border border-[var(--notch-line)] p-2.5">
              <div className="flex items-center justify-between gap-2">
                <h4 className="text-[12px] font-semibold">Option B — Groq Cloud BYOK (Fast, zero GPU needed)</h4>
                <span className="notch-chip">
                  Key: {data.has_groq_key ? `Stored (${data.groq_key_source.toUpperCase()})` : "Not configured"}
                </span>
              </div>
              <p className="notch-muted mt-1 text-[11px]">
                Bring your own free-tier or paid Groq key. Tessera masks names, emails, phones, addresses, DOBs, and
                credentials before sending prompts to <code>api.groq.com</code>. Keys are protected locally with Windows DPAPI.
              </p>

              <div className="mt-2 grid gap-1.5 text-[11px]">
                <label className="flex items-center gap-2">
                  <span className="w-24 shrink-0">Model:</span>
                  <input
                    value={groqModel}
                    onChange={(e) => setGroqModel(e.target.value)}
                    placeholder="openai/gpt-oss-20b"
                    className="notch-input flex-1 rounded px-2 py-1"
                  />
                </label>
                <label className="flex items-center gap-2">
                  <span className="w-24 shrink-0">API Key:</span>
                  <input
                    type="password"
                    autoComplete="off"
                    value={groqKey}
                    onChange={(e) => setGroqKey(e.target.value)}
                    placeholder={data.has_groq_key ? "Stored securely (enter only to replace)" : "Paste gsk_... key"}
                    className="notch-input flex-1 rounded px-2 py-1"
                  />
                </label>
                <label className="mt-0.5 flex items-start gap-1.5">
                  <input type="checkbox" checked={consent} onChange={(e) => setConsent(e.target.checked)} />
                  <span>I understand PII-masked context is sent to Groq Cloud when Groq is selected as the active provider.</span>
                </label>
              </div>

              <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px]">
                <button type="button" className="notch-button font-semibold" disabled={busy || !data.groq_feature_supported} onClick={saveGroq}>
                  {data.provider === "groq" ? "Update Groq Settings" : "Save & Use Groq"}
                </button>
                <button type="button" className="notch-button" disabled={busy || !data.groq_feature_supported} onClick={testGroq}>
                  Test connection
                </button>
                {data.groq_key_source === "dpapi" && (
                  <button type="button" className="notch-button" disabled={busy} onClick={removeGroq}>
                    Remove saved key
                  </button>
                )}
              </div>

              {probe && (
                <p role="status" className="mt-1.5 text-[11px]">
                  {probe.ok
                    ? `Connection succeeded (${probe.latency_ms} ms round-trip).`
                    : `Connection failed (${probe.code ?? "error"}).`}
                </p>
              )}
            </section>
          </>
        )}
      </Status>
    </div>
  );
}
