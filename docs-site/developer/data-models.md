# Data Models & Types

This page defines the primary domain types and data models used throughout Tessera's extraction and storage pipelines.

---

## 1. The Fact Kinds

Atomic intelligence extracted from screen sessions is categorized under five standardized kinds (`tsr_pipeline::schema`):

| Fact Kind | Description | Example Extracted Content |
|---|---|---|
| `action` | Explicit task or commitment assigned to a person or the user | *"Submit Q3 engineering budget by Thursday"* |
| `decision` | Formal consensus, architectural choice, or agreement reached | *"Adopted SQLCipher 4 for local encrypted store"* |
| `event` | Scheduled meeting, deadline, or calendar occurrence | *"Sprint retro scheduled for Friday at 10:00 AM"* |
| `note` | Informational fact, reference, or context snippet | *"Production database cluster endpoint is db.internal"* |
| `question` | Open, unanswered inquiry requiring follow-up | *"What is the expected context size for Qwen3-VL?"* |

---

## 2. Fact Status Lifecycle

Facts transition through an explicit lifecycle:
- **`open`:** Active task or pending item presented in the Open Loops view.
- **`completed`:** Marked done by the user or substantiated by completion evidence.
- **`dismissed`:** Explicitly dismissed by the user.
- **`needs_review`:** Injected command or ambiguous financial transfer flagged for manual review.

---

## 3. Evidence Struct

Every fact must substantiate its claim with exact character offsets in a captured item:

```rust
pub struct Evidence {
    pub fact_id: i64,
    pub item_id: i64,
    pub quote: String,
    pub start_ch: usize,
    pub end_ch: usize,
}
```
- `quote` must match `item.raw_text[start_ch..end_ch]`.
- The database enforces a `UNIQUE(fact_id, item_id, start_ch, end_ch)` constraint to prevent duplicate citations.
