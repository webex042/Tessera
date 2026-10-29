@ABSTRACT
Modern artificial intelligence assistants that understand what a person is working on almost always do so by sending the contents of the screen to a remote server. That design is convenient for the vendor and uncomfortable for the user, because the most useful context is also the most sensitive. This thesis presents Tessera, a desktop system for Windows that attempts the opposite arrangement: the language model runs on the user's own machine, the memory it builds never leaves that machine, and every statement the system makes can be traced back to the exact words it was derived from.

Tessera is built as a Rust workspace of ten libraries behind a desktop shell, totalling about thirty-seven thousand lines of Rust and two thousand lines of TypeScript. It reads text from the window the user is currently working in through a three-stage acquisition ladder — accessibility interfaces first, optical character recognition second, and a vision-language model only when the first two fail — and it holds screen images in memory only, never on disk. Before anything is stored, a deterministic redaction stage removes passwords, one-time codes, card numbers and Indian identity numbers, using checksum validation rather than pattern matching alone. What survives is written to a whole-database-encrypted store whose key is wrapped with the user's Windows login.

Two engineering problems shape the design. The first is hardware: a four-billion-parameter vision-language model must run across unrelated GPU vendors without the application linking to any vendor library. Tessera solves this by keeping each inference backend outside its own binary as a separately pinned, hash-verified build, detecting the installed hardware, probing each build for real, and falling back in a defined order that always ends at the processor. The second problem is authority: text read from a screen may itself contain instructions aimed at the model. Tessera treats that text strictly as data — the model has no tools, its output is constrained by a grammar, and deterministic code then verifies that every proposed fact quotes the source verbatim before anything is stored.

The system was evaluated against release criteria fixed in advance, on an AMD integrated GPU rather than the intended reference machine. The guarantees that depend on code hold: 355 of 355 seeded secrets were removed at a 0.41 per cent false-positive rate, no plaintext was recoverable from disk, no stored fact carried evidence absent from its source, no citation was fabricated, and the floating interface was wholly absent from two independent screen-capture paths. The guarantees that depend on the model or on hardware do not: extraction scored an F1 of 0.533 against a target of 0.70, answers were correct 66.7 per cent of the time against a target of 80 per cent, interactive latency was roughly ten times the target on integrated graphics, and a held-out injection corpus showed that twelve of thirty-nine unseen attacks were still stored as readable to-do entries, though none caused an action or revealed a secret. An outbound connection made by the Microsoft WebView2 runtime, outside the application's control, means the zero-network claim cannot presently be made for the whole process tree.

The contribution of this work is therefore not a finished product but a measured one: an architecture in which privacy and provenance are enforced by code rather than promised by a model, together with an honest account of which of its guarantees survived contact with a real small model on commodity hardware and which did not.

@KEYWORDS
Local inference; privacy-preserving systems; screen-aware memory; retrieval-augmented generation; indirect prompt injection; GPU vendor independence; encrypted local storage; Rust; Windows desktop applications

@ACKNOWLEDGEMENT
I would like to express my sincere gratitude to my project guide, Dr. Ayan Kumar Das, for his guidance, patience and encouragement throughout this work. His questions repeatedly pushed me to look for evidence rather than to settle for a plausible explanation, and the discipline of measuring claims rather than asserting them — which shapes this entire thesis — comes directly from that guidance.

I am grateful to the Department of Computer Science and Engineering, Birla Institute of Technology, Mesra, Off-Campus Patna, for providing the academic environment and the facilities that made this project possible.

!!PLACEHOLDER: If the Head of Department, other faculty members, laboratory staff or any department is to be acknowledged by name, please supply the names and exact designations. They have deliberately been left out rather than guessed.

I would also like to thank the open-source community whose work this project builds upon, in particular the contributors to llama.cpp, SQLCipher, Tauri and the Rust ecosystem, whose freely published code made a project of this scope feasible for a single student.

Finally, I thank my family and friends for their support and patience during the course of this work.

@SIGNOFF
Pranshu Ranjan
BTECH/15191/24
