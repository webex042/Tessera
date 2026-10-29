import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

plt.rcParams.update({
    "font.family": "DejaVu Sans", "font.size": 10,
    "axes.edgecolor": "#333333", "axes.linewidth": 0.8,
    "axes.grid": True, "grid.color": "#dddddd", "grid.linewidth": 0.6,
    "axes.axisbelow": True, "figure.facecolor": "white", "savefig.facecolor": "white",
})
DARK, LIGHT = "#4d4d4d", "#bfbfbf"

# Figure 6.1 - per-kind extraction precision/recall/F1
kinds = ["action\n(task+commitment)", "decision", "event", "note", "question"]
P = [0.68, 1.00, 0.50, 0.21, 0.57]
R = [0.69, 0.25, 0.92, 0.33, 0.44]
F = [0.68, 0.40, 0.65, 0.26, 0.50]
x = np.arange(len(kinds)); w = 0.27
fig, ax = plt.subplots(figsize=(7.0, 3.5))
b1 = ax.bar(x - w, P, w, label="Precision", color=LIGHT, edgecolor="#333333", linewidth=0.7)
b2 = ax.bar(x,     R, w, label="Recall",    color="#808080", edgecolor="#333333", linewidth=0.7)
b3 = ax.bar(x + w, F, w, label="F1",        color=DARK, edgecolor="#333333", linewidth=0.7)
for bars in (b1, b2, b3):
    ax.bar_label(bars, fmt="%.2f", fontsize=7.5, padding=2)
ax.axhline(0.70, color="#000000", linestyle="--", linewidth=1.0)
ax.text(len(kinds) - 0.45, 0.72, "MVP F1 gate = 0.70", fontsize=8, ha="right")
ax.set_xticks(x); ax.set_xticklabels(kinds, fontsize=8.5)
ax.set_ylabel("Score"); ax.set_ylim(0, 1.15)
ax.set_yticks(np.arange(0, 1.01, 0.2))
ax.legend(frameon=False, ncol=3, loc="upper left", fontsize=8.5)
ax.spines[["top", "right"]].set_visible(False)
fig.tight_layout(); fig.savefig("fig_6_1.png", dpi=300); plt.close(fig)

# Figure 6.2 - measured latency vs target, log scale
labels = ["Ask:\nfirst sentence", "Ask:\nfull answer", "Draft:\nfirst text", "Draft:\ncomplete"]
measured = [25.155, 25.540, 5.870, 16.372]
target   = [2.5, 10.0, 3.0, 8.0]
x = np.arange(len(labels)); w = 0.35
fig, ax = plt.subplots(figsize=(7.0, 3.4))
b1 = ax.bar(x - w/2, target,   w, label="Target (MVP gate)", color=LIGHT, edgecolor="#333333", linewidth=0.7)
b2 = ax.bar(x + w/2, measured, w, label="Measured (AMD iGPU, Vulkan)", color=DARK, edgecolor="#333333", linewidth=0.7)
ax.bar_label(b1, fmt="%.1f s", fontsize=8, padding=2)
ax.bar_label(b2, fmt="%.1f s", fontsize=8, padding=2)
ax.set_xticks(x); ax.set_xticklabels(labels, fontsize=8.5)
ax.set_ylabel("Seconds (p50)"); ax.set_ylim(0, 31)
ax.legend(frameon=False, fontsize=8.5, loc="upper right")
ax.spines[["top", "right"]].set_visible(False)
fig.tight_layout(); fig.savefig("fig_6_2.png", dpi=300); plt.close(fig)

# Figure 6.3 - injection corpus: tuned vs held-out halves
groups = ["Original set\n(74 cases)", "Tuned half\n(39 cases)", "Held-out half\n(39 cases)"]
sim = [74/74, 39/39, 27/39]
real = [66/74, 33/39, 24/39]
x = np.arange(len(groups)); w = 0.33
fig, ax = plt.subplots(figsize=(6.4, 3.3))
b1 = ax.bar(x - w/2, [v*100 for v in sim],  w, label="Simulated compromised model", color=LIGHT, edgecolor="#333333", linewidth=0.7)
b2 = ax.bar(x + w/2, [v*100 for v in real], w, label="Real Qwen3-VL-4B model",       color=DARK,  edgecolor="#333333", linewidth=0.7)
ax.bar_label(b1, fmt="%.0f%%", fontsize=8, padding=2)
ax.bar_label(b2, fmt="%.0f%%", fontsize=8, padding=2)
ax.set_xticks(x); ax.set_xticklabels(groups, fontsize=8.5)
ax.set_ylabel("Cases passed (%)"); ax.set_ylim(0, 118)
ax.set_yticks(np.arange(0, 101, 20))
ax.legend(frameon=False, fontsize=8.5, loc="lower left")
ax.spines[["top", "right"]].set_visible(False)
fig.tight_layout(); fig.savefig("fig_6_3.png", dpi=300); plt.close(fig)

# Figure 5.3 - scheduler priority and preemption state
fig, ax = plt.subplots(figsize=(6.6, 2.6))
t = np.linspace(0, 10, 1000)
ax.broken_barh([(0.0, 3.2), (6.0, 2.4)], (2.2, 0.6), facecolors=LIGHT, edgecolor="#333333", linewidth=0.7)
ax.broken_barh([(3.2, 2.8)], (1.2, 0.6), facecolors=DARK, edgecolor="#333333", linewidth=0.7)
ax.broken_barh([(8.4, 1.6)], (0.2, 0.6), facecolors="#808080", edgecolor="#333333", linewidth=0.7)
ax.annotate("P0 arrives:\nP1 stream dropped", xy=(3.2, 2.5), xytext=(3.4, 3.3),
            fontsize=8, ha="left", arrowprops=dict(arrowstyle="->", lw=0.9, color="#333333"))
ax.annotate("P1 re-queued in its\nold place, attempt count\nunchanged", xy=(6.0, 2.5), xytext=(6.1, 3.2),
            fontsize=8, ha="left", arrowprops=dict(arrowstyle="->", lw=0.9, color="#333333"))
ax.text(1.6, 2.5, "P1 extraction", ha="center", va="center", fontsize=8.5)
ax.text(4.6, 1.5, "P0 interactive", ha="center", va="center", fontsize=8.5, color="white")
ax.text(7.2, 2.5, "P1 resumes", ha="center", va="center", fontsize=8.5)
ax.text(9.2, 0.5, "P2 rollup", ha="center", va="center", fontsize=8.5, color="white")
ax.set_yticks([2.5, 1.5, 0.5]); ax.set_yticklabels(["P1 queue", "P0 queue", "P2 queue"], fontsize=9)
ax.set_xlabel("Time (illustrative, not to scale)")
ax.set_xticks([]); ax.set_xlim(-0.2, 10.4); ax.set_ylim(0, 4.3)
ax.grid(False); ax.spines[["top", "right", "bottom"]].set_visible(False)
fig.tight_layout(); fig.savefig("fig_5_3.png", dpi=300); plt.close(fig)
print("charts written")
