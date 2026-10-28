import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Tessera',
  titleTemplate: ':title · Tessera',
  description: 'Hardware-aware, local-first AI inference orchestration and screen-aware memory architecture for Windows.',

  appearance: 'force-dark',

  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
    ['meta', { name: 'theme-color', content: '#0A0A0A' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'Tessera · Technical Documentation' }],
    ['meta', { property: 'og:description', content: 'Official systems documentation for Tessera — a local-first AI inference and screen-aware memory platform for Windows.' }],
  ],

  cleanUrls: true,
  lastUpdated: true,

  markdown: {
    theme: {
      light: 'github-dark',
      dark: 'github-dark'
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
    }
  },

  themeConfig: {
    logo: '/favicon.svg',
    siteTitle: 'tessera_',

    search: {
      provider: 'local',
      options: {
        detailedView: true
      }
    },

    nav: [
      { text: 'Overview', link: '/guide/' },
      { text: 'Quickstart', link: '/getting-started/' },
      { text: 'Architecture', link: '/architecture/' },
      { text: 'Core Systems', link: '/modules/' },
      { text: 'AI Runtime', link: '/runtime/' },
      { text: 'Privacy', link: '/privacy/' },
      { text: 'Developer', link: '/developer/' },
      { text: 'Testing', link: '/testing/' },
    ],

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

    socialLinks: [],

    footer: {
      message: 'Tessera — Where Local AI Meets Windows Memory Infrastructure · Research Prototype v0.0.1',
      copyright: '© 2026 Tessera Contributors · Open Source Architecture · Zero Cloud Dependencies'
    }
  }
})
