---
title: Vercel Documentation Hosting Guide
description: Step-by-step production deployment instructions, configuration settings, and routing behavior for hosting Tessera documentation on Vercel.
---

# Vercel Documentation Hosting Guide

The Tessera documentation website (`docs-site/`) is architected for static generation and instant global edge distribution using **Vercel**. It requires zero server runtimes, zero cloud databases, and zero external API keys.

---

## Deployment Architecture Options

There are two supported methods for deploying Tessera docs to Vercel:

### Option A: Root Directory Set to `docs-site` (Recommended)

When connecting the GitHub repository to Vercel, set the **Root Directory** setting in the Vercel dashboard to `docs-site`.

* **Root Directory**: `docs-site`
* **Framework Preset**: `VitePress` (detected automatically)
* **Build Command**: `npm run build`
* **Output Directory**: `.vitepress/dist` (also set in `docs-site/vercel.json`)
* **Install Command**: `npm install` (the docs site has its own `package-lock.json`; it is not part of the pnpm workspace)

In this configuration, Vercel ignores the desktop Rust and React codebase, installing only the documentation dependencies in `docs-site/package.json`.

```mermaid
flowchart LR
    subgraph Repo["Git Repository"]
        R1["src-tauri/"] ~~~ R2["crates/"]
        R3["docs-site/"]
    end
    subgraph Vercel["Vercel Build Pipeline (Option A)"]
        R3 -->|Root: docs-site| V1["npm install"]
        V1 --> V2["npm run build\n(vitepress build)"]
        V2 --> V3[".vitepress/dist/"]
        V3 -->|Global Deploy| V4["Edge CDN\n(*.vercel.app)"]
    end
```

### Option B: Monorepo Root Deployment

If you prefer to deploy from the repository root without changing the Vercel project settings, a root [`vercel.json`](https://github.com/webex042/Tessera/blob/main/vercel.json) is provided at the repository root:

```json
{
  "$schema": "https://openapi.vercel.sh/vercel.json",
  "buildCommand": "cd docs-site && npm install && npm run build",
  "outputDirectory": "docs-site/.vitepress/dist",
  "framework": null,
  "cleanUrls": true,
  "trailingSlash": false
}
```

---

## Step-by-Step Vercel Deployment

### Step 1: Import Repository
1. Navigate to [https://vercel.com/new](https://vercel.com/new).
2. Connect your GitHub account and locate the **Tessera** repository.
3. Click **Import**.

### Step 2: Configure Project Settings
In the **Configure Project** screen:

* **Project Name**: `tessera-docs` (or your preferred name)
* **Framework Preset**: Select **VitePress** (or leave as auto-detect).
* **Root Directory**: Click **Edit** and select `docs-site`. Click **Continue**.
* **Build and Output Settings**:
  * Build Command: `npm run build`
  * Output Directory: `.vitepress/dist`
  * Install Command: `npm install`
* **Node.js Version**: Select **20.x** or **22.x** in Project Settings → General.

```mermaid
sequenceDiagram
    autonumber
    actor Dev as Developer
    participant GH as GitHub
    participant V as Vercel
    participant Edge as Edge CDN

    Dev->>GH: git push origin main
    GH->>V: Webhook trigger (push event)
    V->>V: Detect docs-site/ directory
    V->>V: npm install (VitePress, Vue, Mermaid)
    V->>V: vitepress build
    V->>V: Compile static HTML + generate MiniSearch index
    V->>Edge: Propagate .vitepress/dist to edge nodes
    Edge-->>Dev: Production URL active (preview or live)
```

### Step 3: Environment Variables
**No environment variables need to be set.**  
All search indexes, navigation trees, and syntax themes are built entirely into client-side static assets. Never inject production API secrets or private tokens into the documentation build pipeline.

The status ticker at the top of every page shows build metadata read during `vitepress build` (see `.vitepress/config.ts`):

| Ticker field | Source | If unavailable |
| :--- | :--- | :--- |
| App version | `src-tauri/tauri.conf.json` (outside `docs-site/`) | shows `unknown` |
| Commit | `VERCEL_GIT_COMMIT_SHA` (set by Vercel automatically), else `git rev-parse` | shows `unknown` |
| Docs build date | the build machine's clock | — |

With **Root Directory** set to `docs-site`, the version is only found if the project setting *Include files outside of the root directory in the Build Step* is on (Vercel's default for new projects).

### Step 4: Click Deploy
Click **Deploy**. The build completes in approximately 20 to 45 seconds.

---

## Production Routing & Behavior

VitePress generates clean URLs that pair with Vercel's edge routing:

### 1. Clean URLs & Direct Page Access
* URLs do not require `.html` extensions (e.g., `/architecture/high-level`).
* Both `docs-site/vercel.json` and the root `vercel.json` configure `"cleanUrls": true`.
* Direct navigation or browser refresh on deep routes (e.g., `/runtime/hardware-backends`) resolves directly to the static HTML file at edge without 404 redirects.

### 2. Client-Side Search
* Search runs locally inside the user's browser using `@vitepress/client` MiniSearch.
* All documentation pages, headings, and code titles are indexed during `vitepress build`.
* No external API calls are made to Algolia, OpenAI, or third-party search servers.

### 3. Client-Side Mermaid Diagrams
* Mermaid diagrams are rendered on the client side via `Mermaid.vue`.
* The server-side generation (SSG) compiles raw Mermaid source code into safe data attributes.
* When loaded in the browser, diagrams are drawn in a neutral engineering style: white boxes, black strokes and arrows, grey groups. The site has a single light theme.

### 4. Custom 404 Handling
* VitePress generates a built-in 404 page (`404.html`) drawn as a black-bordered document window like every other page.
* Vercel automatically serves this page for any unmatched request paths.

---

## SEO & Static Assets

| Resource | Path | Verification |
| :--- | :--- | :--- |
| **Favicon** | `docs-site/public/favicon.svg` | Square logo mark: a white T on black |
| **Robots Policy** | `docs-site/public/robots.txt` | Permissive crawling (`Allow: /`) for public developer docs |
| **Open Graph** | Meta tags in `config.ts` | Sets `og:title`, `og:description`, and `og:type` |
| **Theme Color** | `meta[name="theme-color"]` | Set to `#000000` (the black ticker and title bars) |

---

## Preview Deployments & Pull Requests

Vercel automatically creates isolated **Preview Deployments** for every pull request opened against the repository.
* When documentation files in `docs-site/` are updated, Vercel posts a comment with a unique preview URL (e.g., `https://tessera-docs-git-feat-docs-username.vercel.app`).
* Pull request reviewers can inspect diagrams, navigation links, and code copy blocks prior to merging into `main`.

---

## Local Verification Commands

Before committing and pushing documentation changes, run the following verification pipeline locally:

```bash
# 1. Navigate to documentation directory
cd docs-site

# 2. Install dependencies
npm install

# 3. Check colours and internal links
npm run check:palette
npm run check:links

# 4. Test production static build
npm run build

# 5. Preview the production build locally (starts local HTTP server on http://localhost:4173)
npm run preview
```
