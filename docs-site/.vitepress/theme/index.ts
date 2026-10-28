import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import { h } from 'vue'

// Self-hosted fonts (no request to a font CDN).
import '@fontsource-variable/jetbrains-mono'
import '@fontsource-variable/inter'

import './tokens.css'
import './vars.css'
import './style.css'

import HomeHero from './components/HomeHero.vue'
import HomeSections from './components/HomeSections.vue'
import Mermaid from './components/Mermaid.vue'
import StatusBadge from './components/StatusBadge.vue'

export default {
  extends: DefaultTheme,
  Layout: () =>
    h(DefaultTheme.Layout, null, {
      'home-hero-before': () => h(HomeHero),
      'home-features-after': () => h(HomeSections),
    }),
  enhanceApp({ app }) {
    app.component('Mermaid', Mermaid)
    app.component('StatusBadge', StatusBadge)
  },
} satisfies Theme
