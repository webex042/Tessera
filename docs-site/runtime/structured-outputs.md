# Structured Grammar Outputs

Small 4B parameter models are prone to syntax degradation when generating complex JSON structures. Tessera resolves this by constraining generation to formal execution grammars.

---

## 1. Schema-Constrained Generation

When submitting an extraction request, Tessera specifies a formal JSON schema in the request payload:

```json
{
  "messages": [...],
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "extraction_v1",
      "strict": true,
      "schema": {
        "type": "object",
        "properties": {
          "facts": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "kind": { "type": "string", "enum": ["action", "decision", "event", "note", "question"] },
                "text": { "type": "string" },
                "evidence": { "type": "string" }
              },
              "required": ["kind", "text", "evidence"]
            }
          }
        },
        "required": ["facts"]
      }
    }
  }
}
```

- **Grammar Conversion:** `llama-server` converts the JSON schema into a Context-Free Grammar (CFG) mask.
- **Logit Masking:** During every token generation step, logits corresponding to tokens that would violate the grammar are masked to $-\infty$. The model physically cannot emit malformed JSON or unlisted enum variants.

---

## 2. Double Validation

While grammar constraints prevent syntax errors, they cannot prevent factual inaccuracies. Tessera therefore passes all generated output through deterministic Rust validation:
1. **Schema Deserialization:** Parsed via `serde_json` into strongly-typed domain structs (`tsr_pipeline::schema`).
2. **Evidence Substring Check:** Confirms that the `evidence` field is present within the original captured screen text.
3. **Redaction Check:** Ensures no unredacted secret copied by the model reaches the database.
