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
| **Aadhaar Numbers** | `[AADHAAR]` | 12-digit UID and 16-digit VID validated via Verhoeff base-10 dihedral checksum |
| **PAN Identifiers** | `[PAN]` | 10-character Indian PAN alphanumeric structure checksum |
| **Payment Cards** | `[CARD]` | Luhn checksum algorithm (Visa, Mastercard, Amex, Discover) |
| **Bank Accounts & IFSC** | `[BANK_ACCOUNT]`, `[IFSC]` | Contextual account numbers, 11-character RBI IFSC branch codes, IBAN checksum |
| **Physical Addresses** | `[ADDRESS]` | Street, avenue, sector, building, apartment patterns with postal code heuristics |
| **Dates of Birth** | `[DATE_OF_BIRTH]` | DOB / Birth date context markers paired with calendar date formats |
| **Passwords & Secrets** | `[PASSWORD]`, `[SECRET]` | Keyword-value prompts (`password: ...`), high-entropy secret patterns |
| **API Keys & Credentials** | `[API_KEY]`, `[JWT]` | AWS access keys (`AKIA...`), OpenAI tokens (`sk-...`), GitHub PATs (`ghp_...`), JWTs |
| **OTPs & Verification Codes** | `[OTP]` | 4-8 digit numeric codes paired with English and Hinglish verification cues |

### Deterministic Pseudonym Mapping
To maintain readability and conversational coherence without exposing personal identities, repeated occurrences of personal names within a document context receive deterministic pseudonyms:
- `Alice Smith` $\rightarrow$ `[PERSON_01]`
- Subsequent mentions of `Alice` $\rightarrow$ `[PERSON_01]`
- `Bob Jones` $\rightarrow$ `[PERSON_02]`

Original identities are never stored in reversible token tables or exposed via pseudonym generation.

### Algorithmic Checksum Verification
To prevent excessive false positives while guaranteeing high detection recall, regex matches are verified with domain checksum algorithms:
- **Credit Cards:** Validated with the Luhn checksum algorithm (supports Visa, Mastercard, Amex, Discover).
- **Indian Aadhaar Numbers:** 12-digit and 16-digit Virtual IDs (VID) validated with the Verhoeff base-10 dihedral checksum algorithm.
- **Indian PAN:** 10-character alphanumeric format validated against official structural checksum patterns.
- **Passports & Voter IDs:** Standard format verification.
- **API Keys & Cryptographic Tokens:** High-entropy token matchers for AWS access keys (`AKIA...`), OpenAI keys (`sk-...`), GitHub tokens (`ghp_...`), Slack tokens, and PEM private keys (`-----BEGIN PRIVATE KEY-----`).

### Verification Scorecard
Evaluated against the versioned secret corpus (`tests/fixtures/redaction/`):
- **Detection Rate:** $355 / 355$ ($100.0\%$) of seeded secrets redacted.
- **False Positive Rate:** $1 / 235$ ($0.43\%$) on the clean English Wikipedia test corpus (target: $\le 2.0\%$).
