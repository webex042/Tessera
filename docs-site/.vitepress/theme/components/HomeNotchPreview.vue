<script setup lang="ts">
import { computed, ref } from 'vue'

type TabId = 'today' | 'ask' | 'privacy' | 'setup'
type ProviderMode = 'local' | 'groq'

interface SyntheticBrief {
  id: string
  time: string
  app: string
  summary: string
  quote: string
  factKind: string
}

interface SyntheticPrompt {
  id: string
  question: string
  localAnswer: string
  groqAnswer: string
  sourceApp: string
  snippet: string
}

const activeTab = ref<TabId>('today')
const provider = ref<ProviderMode>('local')
const selectedBriefId = ref<string>('brief-1')
const selectedPromptId = ref<string>('q1')
const retentionDays = ref<number>(30)
const simulateGroqKeyStored = ref<boolean>(false)
const blocklist = ref([
  { id: '1p', label: '1password.exe', kind: 'process', enabled: true },
  { id: 'bw', label: 'bitwarden.exe', kind: 'process', enabled: true },
  { id: 'bank', label: 'Online Banking & Checkout Titles', kind: 'title', enabled: true },
  { id: 'incog', label: 'Private / Incognito Browsing Windows', kind: 'state', enabled: true },
])

const syntheticBriefs: SyntheticBrief[] = [
  {
    id: 'brief-1',
    time: '09:42',
    app: 'Linear — Desktop Shell Sprint',
    factKind: 'Task',
    summary: 'Verify SHA-256 checksums for Tessera Windows x64 portable archive before publishing release notes.',
    quote: '[PERSON_01] assigned: Generate SHA256SUMS.txt and verify llama.cpp sidecar hash against tools/sidecar.lock.',
  },
  {
    id: 'brief-2',
    time: '10:15',
    app: 'VS Code — vectordb.rs',
    factKind: 'Decision',
    summary: 'Migrate vector storage blocks to ring::aead::AES_256_GCM with random 96-bit nonces and .v1.bak migration.',
    quote: 'All vectors stored in tessera_vectors.enc now authenticate header + 12-byte CSPRNG nonce via AAD.',
  },
  {
    id: 'brief-3',
    time: '11:08',
    app: 'Mail — Security Review Notes',
    factKind: 'Follow-up',
    summary: 'Confirm Aadhaar, PAN, Luhn card numbers, and tokens mask to typed tags before Groq BYOK API requests.',
    quote: 'Contact [PERSON_02] at [EMAIL] before Friday to archive audit evidence.',
  },
]

const syntheticPrompts: SyntheticPrompt[] = [
  {
    id: 'q1',
    question: 'What storage security checks were finalized this morning?',
    localAnswer:
      'Vector embeddings moved to AES-256-GCM authenticated encryption in tessera_vectors.enc with CSPRNG nonces, backed by automatic .v1.bak migration from v1 blocks.',
    groqAnswer:
      '[Groq BYOK — masked prompt sent] Summary grounded in local VectorDb hits: tessera_vectors.enc uses AES-256-GCM AEAD (v2) alongside SQLCipher relational metadata, with [PERSON_01] reviewing SHA-256 manifests.',
    sourceApp: 'VS Code — vectordb.rs',
    snippet: 'All vectors stored in vectors.db now authenticate header + 12-byte CSPRNG nonce via AAD.',
  },
  {
    id: 'q2',
    question: 'Which PII safeguards run before optional cloud requests?',
    localAnswer:
      'tsr-guard masks personal names ([PERSON_01]), emails ([EMAIL]), phones ([PHONE]), Aadhaar, PAN, payment cards, JWTs, and API keys before prompts leave the PC.',
    groqAnswer:
      '[Groq BYOK — masked prompt sent] tsr_guard::redact::mask_pii replaces identifiers with typed placeholders ([PERSON_01], [EMAIL], [AADHAAR], [PAN], [API_KEY]) prior to https://api.groq.com dispatch.',
    sourceApp: 'Mail — Security Review Notes',
    snippet: 'Contact [PERSON_02] at [EMAIL] before Friday to archive audit evidence.',
  },
]

const selectedBrief = computed(
  () => syntheticBriefs.find((b) => b.id === selectedBriefId.value) ?? syntheticBriefs[0],
)

const selectedPrompt = computed(
  () => syntheticPrompts.find((p) => p.id === selectedPromptId.value) ?? syntheticPrompts[0],
)

const activeAnswer = computed(() =>
  provider.value === 'local' ? selectedPrompt.value.localAnswer : selectedPrompt.value.groqAnswer,
)

function toggleBlockRule(id: string) {
  const item = blocklist.value.find((r) => r.id === id)
  if (item) item.enabled = !item.enabled
}
</script>

<template>
  <section class="tnp-wrap" aria-label="Interactive Synthetic Notch UI Preview">
    <div class="tnp-banner">
      <span class="tnp-banner-tag">SYNTHETIC BROWSER PREVIEW</span>
      <span class="tnp-banner-note">
        Interactive static mockup using synthetic sample data — not connected to the Windows desktop runtime or any external API.
      </span>
    </div>

    <div class="tnp-frame">
      <header class="tnp-notch-bar">
        <div class="tnp-notch-brand">
          <span class="tnp-dot" aria-hidden="true"></span>
          <span class="tnp-title">TESSERA NOTCH</span>
          <span class="tnp-pill">{{ provider === 'local' ? 'LOCAL · QWEN3-VL-4B' : 'GROQ BYOK · GPT-OSS-20B' }}</span>
          <span class="tnp-pill">VECTORDB · AES-256-GCM</span>
        </div>
        <div class="tnp-tabs" role="tablist" aria-label="Preview Views">
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'today'"
            :class="['tnp-tab', { active: activeTab === 'today' }]"
            @click="activeTab = 'today'"
          >
            Today
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'ask'"
            :class="['tnp-tab', { active: activeTab === 'ask' }]"
            @click="activeTab = 'ask'"
          >
            Ask
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'privacy'"
            :class="['tnp-tab', { active: activeTab === 'privacy' }]"
            @click="activeTab = 'privacy'"
          >
            Privacy
          </button>
          <button
            type="button"
            role="tab"
            :aria-selected="activeTab === 'setup'"
            :class="['tnp-tab', { active: activeTab === 'setup' }]"
            @click="activeTab = 'setup'"
          >
            AI Setup
          </button>
        </div>
      </header>

      <div v-if="activeTab === 'today'" class="tnp-body" role="tabpanel">
        <div class="tnp-col">
          <div class="tnp-section-label">DAILY CAPTURE BRIEFS (SYNTHETIC)</div>
          <ul class="tnp-list">
            <li v-for="b in syntheticBriefs" :key="b.id">
              <button
                type="button"
                :class="['tnp-item-btn', { selected: selectedBriefId === b.id }]"
                @click="selectedBriefId = b.id"
              >
                <div class="tnp-item-top">
                  <span class="tnp-meta">{{ b.time }} · {{ b.factKind.toUpperCase() }}</span>
                  <span class="tnp-app">{{ b.app }}</span>
                </div>
                <p class="tnp-item-text">{{ b.summary }}</p>
              </button>
            </li>
          </ul>
        </div>
        <aside class="tnp-evidence">
          <div class="tnp-section-label">DETERMINISTIC EVIDENCE DRAWER</div>
          <div class="tnp-card">
            <div class="tnp-meta">SOURCE WINDOW</div>
            <div class="tnp-strong">{{ selectedBrief.app }}</div>
            <div class="tnp-meta tnp-mt">MASKED QUOTE EXCERPT</div>
            <blockquote class="tnp-quote">{{ selectedBrief.quote }}</blockquote>
            <div class="tnp-footnote">
              PII tags such as <code>[PERSON_01]</code> and <code>[EMAIL]</code> are applied before storage indexing and prompt construction.
            </div>
          </div>
        </aside>
      </div>

      <div v-else-if="activeTab === 'ask'" class="tnp-body" role="tabpanel">
        <div class="tnp-col">
          <div class="tnp-section-label">SAMPLE GROUNDED QUERIES</div>
          <div class="tnp-starters">
            <button
              v-for="p in syntheticPrompts"
              :key="p.id"
              type="button"
              :class="['tnp-starter', { selected: selectedPromptId === p.id }]"
              @click="selectedPromptId = p.id"
            >
              {{ p.question }}
            </button>
          </div>
          <div class="tnp-provider-row">
            <span class="tnp-meta">SIMULATED ROUTE:</span>
            <button
              type="button"
              :class="['tnp-chip', { active: provider === 'local' }]"
              @click="provider = 'local'"
            >
              Local llama.cpp (Zero Cloud)
            </button>
            <button
              type="button"
              :class="['tnp-chip', { active: provider === 'groq' }]"
              @click="provider = 'groq'"
            >
              Groq BYOK (PII-Masked Outbound)
            </button>
          </div>
          <div class="tnp-card">
            <div class="tnp-meta">SYNTHETIC ANSWER ({{ provider === 'local' ? 'LOCAL' : 'GROQ BYOK' }})</div>
            <p class="tnp-answer">{{ activeAnswer }}</p>
          </div>
        </div>
        <aside class="tnp-evidence">
          <div class="tnp-section-label">HYBRID RETRIEVAL HIT</div>
          <div class="tnp-card">
            <div class="tnp-meta">VECTORDB + FTS5 RRF MATCH</div>
            <div class="tnp-strong">{{ selectedPrompt.sourceApp }}</div>
            <blockquote class="tnp-quote tnp-mt">{{ selectedPrompt.snippet }}</blockquote>
            <div class="tnp-footnote">
              Desktop app performs local cosine similarity in VectorDb merged via Reciprocal Rank Fusion with SQLCipher FTS5.
            </div>
          </div>
        </aside>
      </div>

      <div v-else-if="activeTab === 'privacy'" class="tnp-body" role="tabpanel">
        <div class="tnp-col">
          <div class="tnp-section-label">CAPTURE BLOCKLIST PREVIEW</div>
          <ul class="tnp-list">
            <li v-for="rule in blocklist" :key="rule.id" class="tnp-rule-row">
              <div>
                <div class="tnp-strong">{{ rule.label }}</div>
                <div class="tnp-meta">RULE TYPE: {{ rule.kind.toUpperCase() }}</div>
              </div>
              <button type="button" class="tnp-chip" :class="{ active: rule.enabled }" @click="toggleBlockRule(rule.id)">
                {{ rule.enabled ? 'Blocked' : 'Allowed' }}
              </button>
            </li>
          </ul>
        </div>
        <aside class="tnp-evidence">
          <div class="tnp-section-label">RETENTION &amp; WIPE CONTROLS</div>
          <div class="tnp-card">
            <div class="tnp-meta">AUTO-PRUNE RETENTION WINDOW</div>
            <div class="tnp-provider-row tnp-mt">
              <button
                v-for="days in [7, 30, 90]"
                :key="days"
                type="button"
                :class="['tnp-chip', { active: retentionDays === days }]"
                @click="retentionDays = days"
              >
                {{ days }} days
              </button>
            </div>
            <p class="tnp-footnote tnp-mt">
              Desktop Privacy settings purge aged rows from SQLCipher and orphan vectors from <code>tessera_vectors.enc</code> hourly and at startup, and support token-confirmed full memory wipes.
            </p>
          </div>
        </aside>
      </div>

      <div v-else class="tnp-body" role="tabpanel">
        <div class="tnp-col">
          <div class="tnp-section-label">RUNTIME &amp; MODEL STATUS (SYNTHETIC)</div>
          <div class="tnp-card">
            <div class="tnp-row-between">
              <span class="tnp-strong">Local Sidecar Runtime</span>
              <span class="tnp-pill">VULKAN / CPU AVX2 · VERIFIED</span>
            </div>
            <p class="tnp-footnote tnp-mt">
              Pinned <code>llama.cpp</code> b11900 builds and <code>onnxruntime.dll</code> ship SHA-256 verified inside the Windows installer. Model weights download only on user consent.
            </p>
            <div class="tnp-row-between tnp-mt">
              <span class="tnp-strong">Groq API Key (Windows DPAPI)</span>
              <button type="button" class="tnp-chip" :class="{ active: simulateGroqKeyStored }" @click="simulateGroqKeyStored = !simulateGroqKeyStored">
                {{ simulateGroqKeyStored ? 'Simulated: Stored in DPAPI' : 'Simulated: Not Configured' }}
              </button>
            </div>
          </div>
        </div>
        <aside class="tnp-evidence">
          <div class="tnp-section-label">PROVIDER SELECTION RULES</div>
          <div class="tnp-card">
            <ul class="tnp-checklist">
              <li>Default runtime starts in strict Local-only mode.</li>
              <li>Zero API keys bundled in source, installer, or website.</li>
              <li>Switching to Groq requires your own key + PII redaction notice.</li>
              <li>Keys stay local inside Windows DPAPI keystore.</li>
            </ul>
          </div>
        </aside>
      </div>
    </div>
  </section>
</template>

<style scoped>
.tnp-wrap {
  max-width: 1120px;
  margin: 0 auto 2.5rem auto;
  border: 1px solid #111111;
  background: #FFFFFF;
  color: #111111;
  font-family: inherit;
}

.tnp-banner {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
  padding: 0.65rem 1rem;
  background: #111111;
  color: #FFFFFF;
  border-bottom: 1px solid #111111;
  font-size: 0.78rem;
}

.tnp-banner-tag {
  font-weight: 700;
  letter-spacing: 0.08em;
  padding: 0.15rem 0.5rem;
  border: 1px solid #FFFFFF;
}

.tnp-banner-note {
  color: #E8E8E8;
}

.tnp-frame {
  background: #F5F5F5;
}

.tnp-notch-bar {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 0.75rem;
  padding: 0.85rem 1rem;
  background: #FFFFFF;
  border-bottom: 1px solid #D8D8D8;
}

.tnp-notch-brand {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.5rem;
}

.tnp-dot {
  width: 8px;
  height: 8px;
  background: #000000;
  display: inline-block;
}

.tnp-title {
  font-weight: 700;
  font-size: 0.85rem;
  letter-spacing: 0.06em;
}

.tnp-pill {
  font-size: 0.7rem;
  padding: 0.15rem 0.45rem;
  background: #F2F2F2;
  border: 1px solid #D8D8D8;
  color: #1A1A1A;
  font-weight: 600;
}

.tnp-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}

.tnp-tab,
.tnp-chip {
  cursor: pointer;
  padding: 0.35rem 0.75rem;
  font-size: 0.8rem;
  font-weight: 600;
  border: 1px solid #111111;
  background: #FFFFFF;
  color: #111111;
}

.tnp-tab.active,
.tnp-chip.active {
  background: #000000;
  color: #FFFFFF;
}

.tnp-body {
  display: grid;
  grid-template-columns: 1.3fr 1fr;
  gap: 1rem;
  padding: 1rem;
}

@media (max-width: 820px) {
  .tnp-body {
    grid-template-columns: 1fr;
  }
}

.tnp-section-label {
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: #777777;
  margin-bottom: 0.5rem;
}

.tnp-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.tnp-item-btn {
  width: 100%;
  text-align: left;
  cursor: pointer;
  padding: 0.75rem;
  background: #FFFFFF;
  border: 1px solid #D8D8D8;
  color: #111111;
}

.tnp-item-btn.selected {
  border-color: #000000;
  box-shadow: inset 3px 0 0 #000000;
}

.tnp-item-top,
.tnp-row-between {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.5rem;
}

.tnp-meta {
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0.05em;
  color: #777777;
}

.tnp-app {
  font-size: 0.75rem;
  color: #1A1A1A;
}

.tnp-item-text,
.tnp-answer {
  margin: 0.35rem 0 0 0;
  font-size: 0.88rem;
  line-height: 1.45;
  color: #111111;
}

.tnp-card {
  background: #FFFFFF;
  border: 1px solid #D8D8D8;
  padding: 0.85rem;
}

.tnp-strong {
  font-weight: 700;
  font-size: 0.88rem;
  color: #111111;
}

.tnp-quote {
  margin: 0.35rem 0 0 0;
  padding: 0.5rem 0.75rem;
  background: #F2F2F2;
  border-left: 3px solid #111111;
  font-size: 0.82rem;
  color: #1A1A1A;
}

.tnp-footnote {
  margin-top: 0.65rem;
  font-size: 0.78rem;
  color: #777777;
  line-height: 1.4;
}

.tnp-mt {
  margin-top: 0.65rem;
}

.tnp-starters {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  margin-bottom: 0.75rem;
}

.tnp-starter {
  text-align: left;
  cursor: pointer;
  padding: 0.55rem 0.75rem;
  background: #FFFFFF;
  border: 1px solid #D8D8D8;
  color: #111111;
  font-size: 0.84rem;
}

.tnp-starter.selected {
  border-color: #000000;
  background: #F2F2F2;
  font-weight: 600;
}

.tnp-provider-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.45rem;
  margin-bottom: 0.75rem;
}

.tnp-rule-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 0.75rem;
  padding: 0.65rem 0.75rem;
  background: #FFFFFF;
  border: 1px solid #D8D8D8;
}

.tnp-checklist {
  margin: 0;
  padding-left: 1.1rem;
  font-size: 0.83rem;
  line-height: 1.55;
  color: #1A1A1A;
}
</style>
