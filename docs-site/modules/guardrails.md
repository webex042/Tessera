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

## 2. Redaction Engine (`tsr_guard::redact`)

Scrubs sensitive credentials and personal data before text reaches SQLCipher or model prompts:

```
Raw Text Stream ──► Regex Heuristic ──► Algorithmic Checksum ──► [REDACTED:KIND]
```

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
