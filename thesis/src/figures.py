# Figure registry for the Tessera thesis.
# id -> (filename, caption, width in inches)

FIGURES = {
"3.1": ("fig_3_1.png", "Use-case diagram for Tessera. Every case is initiated by the single desktop user; the system performs no action on its own behalf.", 6.0),
"4.1": ("fig_4_1.png", "High-level architecture. Three process boundaries separate the interface, the orchestration host and the inference worker; the hardware backend is chosen at run time.", 6.0),
"4.2": ("fig_4_2.png", "Sequence of an interactive inference request, including worker start-up and the preemption of a background job.", 6.0),
"4.3": ("fig_4_3.png", "Data acquisition and privacy pipeline. Each gate can only reduce what is captured; nothing bypasses redaction on the way to storage.", 6.0),
"4.4": ("fig_4_4.png", "Backend selection and fallback. Detection, verification, probing and model inspection all precede any attempt to launch a worker.", 6.0),
"4.5": ("fig_4_5.png", "Entity-relationship diagram of the encrypted store, as created by the four forward-only migrations.", 6.0),
"5.1": ("fig_5_1.png", "Extraction and deterministic validation. The model proposes; code decides what is stored, and in what state.", 6.0),
"5.2": ("fig_5_2.png", "Retrieval and grounded question answering, from query parsing to per-sentence citation verification.", 6.0),
"4.6": ("fig_5_3.png", "Scheduler behaviour when an interactive request arrives while a background extraction job is streaming.", 6.0),
"6.2": ("fig_6_1.png", "Extraction precision, recall and F1 by fact kind on the held-out set, real model run.", 6.0),
"6.3": ("fig_6_2.png", "Measured median latency against the project's own interactive targets, on the available integrated GPU.", 6.0),
"6.1": ("fig_6_3.png", "Injection corpus pass rates. The held-out half, which was never used for tuning, is the honest estimate of generalisation.", 6.0),
}

# Short titles used in the List of Figures.
SHORT = {
"3.1": "Use-case diagram for Tessera",
"4.1": "High-level architecture",
"4.2": "Sequence of an interactive inference request",
"4.3": "Data acquisition and privacy pipeline",
"4.4": "Backend selection and fallback",
"4.5": "Entity-relationship diagram of the encrypted store",
"4.6": "Scheduler preemption of a background job",
"5.1": "Extraction and deterministic validation",
"5.2": "Retrieval and grounded question answering",
"6.1": "Injection corpus pass rates",
"6.2": "Extraction precision, recall and F1 by fact kind",
"6.3": "Measured median latency against interactive targets",
}
