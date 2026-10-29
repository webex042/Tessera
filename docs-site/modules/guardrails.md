# Guardrails & Redaction (`tsr-guard`)

The `tsr-guard` crate provides deterministic, platform-neutral security controls. It compiles without Windows dependencies to enable fast, cross-platform unit testing.

---

## 1. Application & Window Blocklists

Evaluated on every foreground change event before any text or image capture is initiated:

| Match Category | Target | Built-in Default Rules |
|---|---|---|
| **Password Managers** | Process Name | `1password.exe`, `bitwarden.exe`, `keepass.exe`, `keepassxc.exe`, `dashlane.exe` |
| **Authentication Windows** | Window Title | Titles containing `"sign in"`, `"log in"`, `"verify"`, `"authenticator"` |
| **Banking Portals** | Window Title / URL | Banking domain patterns, `"checkout"`, `"payment gateway"` |
| **Private Browsing** | Window Title & State | Chrome/Edge Incognito, Firefox Private Browsing |

::: tip Private Browsing Fail-Safe
If a web browser's accessibility tree fails to disclose whether a window is in private mode, Tessera's policy engine defaults to **blocked**.
:::

---

## 2. PII Detection & Masking Engine (`tsr_guard::redact`)

Scrubs sensitive personal information, credentials, and identifiers before text reaches model prompts, embedding models, or persistent storage:

```
Raw Text Stream ──► Contextual Pattern Detection ──► Algorithmic Checksum ──► Typed Masking Placeholder
```

### Supported PII Categories & Typed Placeholders

| Category | Placeholder Token | Validation & Detection Mechanism |
|---|---|---|
| **Personal Names** | `[PERSON_01]`, `[PERSON_02]` | Honorifics, context verbs, greetings, with deterministic document-scoped pseudonym mapping |
| **Email Addresses** | `[EMAIL]` | RFC 5322 alphanumeric email pattern with domain validation |
| **Telephone Numbers** | `[PHONE]` | International E.164 and localized Indian/US telephone patterns |
| **Aadhaar Identifiers** | `[AADHAAR]` | 12-digit UID and 16-digit VID validated via Verhoeff base-10 dihedral checksum |
| **PAN Identifiers** | `[PAN]` | 10-character Indian PAN alphanumeric structure checksum |
| **Payment Cards** | `[CARD]` | Luhn checksum algorithm (Visa, Mastercard, Amex, Discover) |
| **Bank Account Identifiers** | `[BANK_ACCOUNT]` | Contextual account numbers, international IBAN format validation |
| **IFSC Identifiers** | `[IFSC]` | 11-character RBI IFSC branch codes |
| **Physical Addresses & Postal Codes** | `[ADDRESS]` | Street, avenue, sector, building, apartment patterns with postal code / PIN heuristics |
| **Dates of Birth** | `[DATE_OF_BIRTH]` | DOB / Birth date context markers paired with calendar date formats |
| **Passwords** | `[PASSWORD]` | Keyword-value prompts (`password: ...`), authentication secret patterns |
| **Generic Secrets** | `[SECRET]` | High-entropy generic secret strings and credential values |
| **API Keys & Access Tokens** | `[API_KEY]` | AWS access keys (`AKIA...`), OpenAI tokens (`sk-...`), GitHub PATs (`ghp_...`), Slack tokens |
| **JSON Web Tokens** | `[JWT]` | Standard three-part base64url encoded tokens (`ey...`) |
| **Private Keys** | `[PRIVATE_KEY]` | PEM private key blocks (`-----BEGIN PRIVATE KEY-----`) |
| **One-Time Passwords** | `[OTP]` | 4-8 digit numeric codes paired with English and Hinglish verification cues |

### The PII Protection Lifecycle

The privacy engine enforces masking across five distinct stages of the data path:

1. **Session Screening:** Raw text captured from active desktop windows is screened through the `tsr-guard` protection layer before entering session buffers.
2. **Pre-Embedding Masking:** PII is masked before text chunking and embedding (`embed_item_to_vectordb`). Embeddings are generated exclusively from sanitized text chunks.
3. **Query Sanitization:** Search queries and question-answering prompts are sanitized with `mask_pii()` before generating query embeddings.
4. **Secondary Redaction Pass:** Retrieved evidence snippets and model-generated responses undergo a secondary redaction pass before display in the UI or inclusion in draft responses.
5. **Typed Placeholders & Scoped Pseudonyms:** Standard typed placeholders and deterministic, document-scoped pseudonyms maintain semantic readability without leaking original identifiers.

The explicit purpose of this architecture is to prevent raw sensitive identifiers from entering model prompts, embedding inputs, and vector-index payloads through any documented application path.

### Deterministic Pseudonym Mapping
To maintain readability and conversational coherence without exposing personal identities, repeated occurrences of personal names within a document context receive deterministic pseudonyms:
- `Alice Smith` $\rightarrow$ `[PERSON_01]`
- Subsequent mentions of `Alice` $\rightarrow$ `[PERSON_01]`
- `Bob Jones` $\rightarrow$ `[PERSON_02]`

Original identities are never stored in reversible token lookup tables or exposed via pseudonym generation.

### Algorithmic Checksum Verification
To prevent excessive false positives while guaranteeing high detection recall, regex matches are verified with domain checksum algorithms:
- **Credit Cards:** Validated with the Luhn checksum algorithm (supports Visa, Mastercard, Amex, Discover).
- **Indian Aadhaar Numbers:** 12-digit and 16-digit Virtual IDs (VID) validated with the Verhoeff base-10 dihedral checksum algorithm.
- **Indian PAN:** 10-character alphanumeric format validated against official structural checksum patterns.
- **Passports & Voter IDs:** Standard format verification.
- **API Keys & Cryptographic Tokens:** High-entropy token matchers for AWS access keys (`AKIA...`), OpenAI keys (`sk-...`), GitHub tokens (`ghp_...`), Slack tokens, and PEM private keys (`-----BEGIN PRIVATE KEY-----`).

### Detection Accuracy & Known Limitations
Tessera makes no claim that all possible PII can be detected under every circumstance, or that automated masking guarantees complete anonymity:
- Contextual name detection relies on linguistic indicators (honorifics, greetings, role prepositions); isolated or uncapitalized names without context cues are intentionally preserved to avoid degrading technical and academic prose.
- Handwritten text, corrupted OCR output, or novel identifier formats outside supported pattern definitions may not be flagged.
- Physical address detection uses bounded structural patterns; non-standard or partial street descriptions may bypass heuristics.

### Verification Scorecard
Evaluated against the versioned secret corpus (`tests/fixtures/redaction/`):
- **Detection Rate:** $355 / 355$ ($100.0\%$) of seeded secrets redacted.
- **False Positive Rate:** $1 / 235$ ($0.43\%$) on the clean English Wikipedia test corpus (target: $\le 2.0\%$).
