import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import { h } from 'vue'

// Self-hosted fonts (no request to a font CDN).
import '@fontsource-variable/jetbrains-mono'
import '@fontsource-variable/inter'

import './tokens.css'
import './vars.css'
import './style.css'

import DocWindowBar from './components/DocWindowBar.vue'
import HomeArchive from './components/HomeArchive.vue'
import Mermaid from './components/Mermaid.vue'
import StatusBadge from './components/StatusBadge.vue'
import StatusTicker from './components/StatusTicker.vue'

export default {
  extends: DefaultTheme,
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      // Black build-information strip above the header (height: --t-ticker-h).
      'layout-top': () => h(StatusTicker),
      // Home page: the project archive (index.md has no hero or features of its own).
      'home-hero-before': () => h(HomeArchive),
      // Every documentation page: window title bar and document header.
      'doc-before': () => h(DocWindowBar),
    }),
  enhanceApp({ app }) {
    app.component('Mermaid', Mermaid)
    app.component('StatusBadge', StatusBadge)
  },
} satisfies Theme
