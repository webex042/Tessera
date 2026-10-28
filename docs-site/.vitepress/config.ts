import { defineConfigWithTheme, type DefaultTheme } from 'vitepress'
import { execSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Build metadata shown in the status ticker. Everything is read at build time from the
// repository (or from Vercel's build environment), so nothing on the ticker is hard-coded.
const here = dirname(fileURLToPath(import.meta.url))

function appVersion(): string {
  try {
    const conf = JSON.parse(readFileSync(resolve(here, '../../src-tauri/tauri.conf.json'), 'utf8'))
    return typeof conf.version === 'string' ? conf.version : 'unknown'
  } catch {
    return 'unknown'
  }
}

function commit(): string {
  const fromVercel = process.env.VERCEL_GIT_COMMIT_SHA
  if (fromVercel) return fromVercel.slice(0, 7)
  try {
    return execSync('git rev-parse --short HEAD', { cwd: here, stdio: ['ignore', 'pipe', 'ignore'] })
      .toString()
      .trim()
  } catch {
    return 'unknown'
  }
}

type BuildInfo = { version: string; commit: string; date: string }

const build: BuildInfo = {
  version: appVersion(),
  commit: commit(),
  date: new Date().toISOString().slice(0, 10),
}

// The default theme's options plus the build metadata read by StatusTicker.vue.
type ThemeConfig = DefaultTheme.Config & { build: BuildInfo }

// Code highlighting in the same black / white / grey palette as the rest of the site:
// structure is shown by weight and tone, not hue.
const archiveCodeTheme = {
  name: 'tessera-archive',
  type: 'dark' as const,
  colors: {
    'editor.background': '#000000',
    'editor.foreground': '#F5F5F5',
  },
  tokenColors: [
    { settings: { foreground: '#F5F5F5' } },
    { scope: ['comment', 'punctuation.definition.comment'], settings: { foreground: '#777777', fontStyle: 'italic' } },
    { scope: ['string', 'string.quoted', 'markup.inline.raw'], settings: { foreground: '#D8D8D8' } },
    {
      scope: ['keyword', 'storage', 'storage.type', 'keyword.control', 'keyword.operator.new', 'support.type.property-name', 'entity.name.tag'],
      settings: { foreground: '#FFFFFF', fontStyle: 'bold' },
    },
    { scope: ['entity.name.function', 'support.function', 'entity.name.type', 'support.type'], settings: { foreground: '#FFFFFF' } },
    { scope: ['constant', 'constant.numeric', 'variable.other.constant'], settings: { foreground: '#E8E8E8' } },
    { scope: ['markup.heading', 'markup.bold'], settings: { foreground: '#FFFFFF', fontStyle: 'bold' } },
  ],
}

// The pages write formulas in TeX ($\le 3.0\%$, $$\frac{a}{b}$$) but the site has no math
// renderer, so they are turned into plain readable text at build time (≤ 3.0%, (a)/(b)).
const MATH_SYMBOLS: Record<string, string> = {
  le: '≤', leq: '≤', ge: '≥', geq: '≥', approx: '≈', sim: '~', pm: '±', times: '×', cdot: '·',
  rightarrow: '→', to: '→', infty: '∞', cap: '∩', cup: '∪', in: '∈', sum: 'Σ', left: '', right: '',
  quad: ' ',
}
const SUPERSCRIPT = '⁰¹²³⁴⁵⁶⁷⁸⁹'

function plainMath(src: string): string {
  let s = src.replace(/\\(?:text|mathrm|mathit|operatorname)\{([^{}]*)\}/g, '$1')
  for (let i = 0; i < 4; i++) s = s.replace(/\\frac\{([^{}]*)\}\{([^{}]*)\}/g, '($1)/($2)')
  s = s
    .replace(/\\([%{}_$#&])/g, '$1')
    .replace(/\\[,;: ]/g, ' ')
    .replace(/\\([a-zA-Z]+)/g, (_m, name: string) => MATH_SYMBOLS[name] ?? name)
    .replace(/\^(\d+)/g, (_m, d: string) => [...d].map((c) => SUPERSCRIPT[Number(c)]).join(''))
    .replace(/\^\{([^{}]*)\}/g, '^($1)')
    .replace(/_\{([^{}]*)\}/g, '_$1')
    .replace(/([A-Za-z])_(\d)/g, '$1$2')
  return s.replace(/\s+/g, ' ').trim()
}

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

export default defineConfigWithTheme<ThemeConfig>({
  title: 'Tessera',
  titleTemplate: ':title · Tessera',
  description: 'Technical documentation for Tessera: a local-first, screen-aware memory assistant for Windows 11 with on-device model inference.',

  appearance: false,

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
    ['meta', { name: 'theme-color', content: '#000000' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'Tessera · Technical Documentation' }],
    ['meta', { property: 'og:description', content: 'Technical documentation for Tessera, a local-first, screen-aware memory assistant for Windows 11 (research prototype).' }],
  ],

  cleanUrls: true,
  lastUpdated: true,

  markdown: {
    theme: {
      light: archiveCodeTheme,
      dark: archiveCodeTheme,
    },
    lineNumbers: true,
    config: (md) => {
      const defaultFence = md.renderer.rules.fence!
      md.renderer.rules.fence = (tokens, idx, options, env, self) => {
        const token = tokens[idx]
        if (token.info.trim() === 'mermaid') {
          const encoded = encodeURIComponent(token.content)
          return `<Mermaid code="${encoded}" />`
        }
        return defaultFence(tokens, idx, options, env, self)
      }

      // Plain-text maths (see plainMath above). Only prose text is touched; code spans and
      // fences are separate tokens and stay exactly as written.
      md.core.ruler.push('tsr-plain-math', (state) => {
        for (const token of state.tokens) {
          if (token.type !== 'inline' || !token.children) continue
          const display = token.content.trim().match(/^\$\$([\s\S]+)\$\$$/)
          if (display) {
            const html = new state.Token('html_inline', '', 0)
            html.content = `<span class="t-formula">${escapeHtml(plainMath(display[1]))}</span>`
            token.children = [html]
            continue
          }
          for (const child of token.children) {
            if (child.type !== 'text' || !child.content.includes('$')) continue
            child.content = child.content.replace(/\$(?=\S)([^$\n]+?)(?<=\S)\$(?!\d)/g, (_m, inner: string) =>
              plainMath(inner),
            )
          }
        }
      })
    }
  },

  themeConfig: {
    logo: { src: '/favicon.svg', alt: '' },
    siteTitle: 'TESSERA',

    // Read by the status ticker (StatusTicker.vue).
    build,

    search: {
      provider: 'local',
      options: {
        detailedView: true,
        translations: {
          button: { buttonText: 'Search docs', buttonAriaLabel: 'Search documentation' },
        },
      },
    },

    nav: [
      { text: 'Overview', link: '/guide/', activeMatch: '^/guide/' },
      { text: 'Getting Started', link: '/getting-started/', activeMatch: '^/getting-started/' },
      { text: 'Architecture', link: '/architecture/', activeMatch: '^/architecture/' },
      { text: 'Core Systems', link: '/modules/', activeMatch: '^/modules/' },
      { text: 'AI Runtime', link: '/runtime/', activeMatch: '^/runtime/' },
      { text: 'Privacy & Storage', link: '/privacy/', activeMatch: '^/privacy/' },
      {
        text: 'Reference',
        activeMatch: '^/(developer|testing|deployment|contributing)/',
        items: [
          { text: 'Developer Reference', link: '/developer/' },
          { text: 'Testing & Quality', link: '/testing/' },
          { text: 'Deployment', link: '/deployment/' },
          { text: 'Contributing', link: '/contributing/' },
        ],
      },
    ],

    outline: { level: [2, 3], label: 'Contents' },
    docFooter: { prev: 'Previous document', next: 'Next document' },
    lastUpdated: { text: 'Last revised' },
    sidebarMenuLabel: 'Documents',
    returnToTopLabel: 'Return to top',
    notFound: {
      title: 'DOCUMENT NOT FOUND',
      quote: 'No page exists at this address. It may have moved, or the link may be mistyped.',
      linkLabel: 'Return to the documentation index',
      linkText: 'Return to index',
      code: '404',
    },

    sidebar: {
      '/guide/': [
        {
          text: 'overview',
          items: [
            { text: 'Introduction', link: '/guide/' },
            { text: 'What Is Tessera', link: '/guide/what-is-tessera' },
            { text: 'Problem & Motivation', link: '/guide/problem-and-motivation' },
            { text: 'Key Concepts', link: '/guide/key-concepts' },
            { text: 'Design Principles', link: '/guide/design-principles' },
            { text: 'Implementation Status', link: '/guide/status' },
            { text: 'System Requirements', link: '/guide/requirements' },
            { text: 'Known Limitations', link: '/guide/limitations' },
            { text: 'Roadmap', link: '/guide/roadmap' },
          ]
        }
      ],
      '/getting-started/': [
        {
          text: 'getting started',
          items: [
            { text: 'Overview', link: '/getting-started/' },
            { text: 'Prerequisites', link: '/getting-started/prerequisites' },
            { text: 'Installation & Setup', link: '/getting-started/installation' },
            { text: 'Development Environment', link: '/getting-started/development' },
            { text: 'Configuration', link: '/getting-started/configuration' },
            { text: 'Running the App', link: '/getting-started/running' },
            { text: 'First-Run Workflow', link: '/getting-started/first-run' },
            { text: 'Production Build', link: '/getting-started/production-build' },
            { text: 'Troubleshooting', link: '/getting-started/troubleshooting' },
          ]
        }
      ],
      '/architecture/': [
        {
          text: 'architecture',
          items: [
            { text: 'System Overview', link: '/architecture/' },
            { text: 'High-Level Architecture', link: '/architecture/high-level' },
            { text: 'Component Breakdown', link: '/architecture/components' },
            { text: 'Startup & Lifecycle', link: '/architecture/lifecycle' },
            { text: 'Process Isolation Model', link: '/architecture/process-model' },
            { text: 'Data Flow', link: '/architecture/data-flow' },
            { text: 'IPC & Event Dispatch', link: '/architecture/ipc' },
            { text: 'Failure & Recovery', link: '/architecture/failure-recovery' },
            { text: 'Dependency Boundaries', link: '/architecture/dependency-boundaries' },
          ]
        }
      ],
      '/modules/': [
        {
          text: 'core systems',
          items: [
            { text: 'Overview', link: '/modules/' },
            { text: 'Desktop Shell (Tauri 2)', link: '/modules/tauri-host' },
            { text: 'OS Integration (tsr-win)', link: '/modules/win-integration' },
            { text: 'Capture Ladder (tsr-capture)', link: '/modules/capture-ladder' },
            { text: 'Guardrails (tsr-guard)', link: '/modules/guardrails' },
            { text: 'Encrypted Storage (tsr-store)', link: '/modules/storage' },
            { text: 'Retrieval (tsr-retrieval)', link: '/modules/retrieval' },
            { text: 'Scheduler (tsr-infer)', link: '/modules/scheduler' },
            { text: 'Pipeline (tsr-pipeline)', link: '/modules/pipeline' },
            { text: 'Drafting (tsr-draft)', link: '/modules/drafting' },
          ]
        }
      ],
      '/runtime/': [
        {
          text: 'ai & inference runtime',
          items: [
            { text: 'Overview', link: '/runtime/' },
            { text: 'Hardware Backends', link: '/runtime/hardware-backends' },
            { text: 'Backend Selection', link: '/runtime/backend-selection' },
            { text: 'Model Validation & GGUF', link: '/runtime/model-validation' },
            { text: 'Sidecar Lifecycle', link: '/runtime/sidecar-lifecycle' },
            { text: 'Loopback Client', link: '/runtime/loopback-client' },
            { text: 'Structured Outputs', link: '/runtime/structured-outputs' },
            { text: 'Memory Sizing & VRAM', link: '/runtime/memory-sizing' },
            { text: 'Performance', link: '/runtime/performance' },
          ]
        }
      ],
      '/privacy/': [
        {
          text: 'data, storage & privacy',
          items: [
            { text: 'Overview', link: '/privacy/' },
            { text: 'Privacy Architecture', link: '/privacy/privacy-architecture' },
            { text: 'Data Lifecycle', link: '/privacy/data-lifecycle' },
            { text: 'Database Schema', link: '/privacy/database-schema' },
            { text: 'DPAPI Key Management', link: '/privacy/key-management' },
            { text: 'Content-Free Logging', link: '/privacy/content-free-logging' },
            { text: 'Threat Model', link: '/privacy/threat-model' },
          ]
        }
      ],
      '/developer/': [
        {
          text: 'developer reference',
          items: [
            { text: 'Overview', link: '/developer/' },
            { text: 'Crate Ecosystem', link: '/developer/crates' },
            { text: 'Configuration Reference', link: '/developer/configuration' },
            { text: 'Rust API Surface', link: '/developer/rust-api' },
            { text: 'IPC Commands & Events', link: '/developer/ipc-commands' },
            { text: 'CLI Tools', link: '/developer/cli-tools' },
            { text: 'Data Models & Types', link: '/developer/data-models' },
            { text: 'ADRs', link: '/developer/adrs' },
            { text: 'Glossary', link: '/developer/glossary' },
            { text: 'FAQ', link: '/developer/faq' },
            { text: 'Licenses', link: '/developer/licenses' },
          ]
        }
      ],
      '/testing/': [
        {
          text: 'testing & quality',
          items: [
            { text: 'Strategy', link: '/testing/' },
            { text: 'Running Tests', link: '/testing/running-tests' },
            { text: 'Hardware Validation', link: '/testing/hardware-tests' },
            { text: 'Redaction Tests', link: '/testing/redaction-tests' },
            { text: 'Injection Corpus', link: '/testing/injection-tests' },
            { text: 'Evaluations & Benchmarks', link: '/testing/evals-and-benchmarks' },
          ]
        }
      ],
      '/deployment/': [
        {
          text: 'deployment & ci/cd',
          items: [
            { text: 'Overview', link: '/deployment/' },
            { text: 'Vercel Hosting', link: '/deployment/vercel-hosting' },
            { text: 'Desktop Packaging', link: '/deployment/packaging' },
            { text: 'Release Engineering', link: '/deployment/release-engineering' },
            { text: 'CI/CD Workflows', link: '/deployment/ci-cd' },
          ]
        }
      ],
      '/contributing/': [
        {
          text: 'contributing',
          items: [
            { text: 'Guide', link: '/contributing/' },
            { text: 'Development Workflow', link: '/contributing/workflow' },
            { text: 'Adding Backends', link: '/contributing/adding-backends' },
            { text: 'Adding Models', link: '/contributing/adding-models' },
          ]
        }
      ],
    },

    footer: {
      message: `Tessera · research prototype v${build.version} · <a href="https://github.com/webex042/Tessera">Source repository</a> · <a href="/guide/status">Implementation status</a> · <a href="/guide/limitations">Known limitations</a>`,
      copyright: 'Project licence: to be determined (README §24) · Third-party notices: THIRD_PARTY_NOTICES.md',
    }
  }
})
