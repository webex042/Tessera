# Prompts and schemas

Versioned prompt templates and JSON schemas (plan §16). Every model output is stored with the
`prompt_ver` it came from (`<kind>/<version>`, e.g. `extraction/v1`). Changing a prompt or schema
means adding a new version directory and including eval results in the PR (plan §23 D).

The Stage 0 benchmark harness (`bench/`) reads these files directly, so what is benchmarked is
what ships. The `tsr-infer` crate that embeds them arrives in Stage 2.

Placeholders use `{name}`. Literal braces in templates are written `{{` and `}}`.
