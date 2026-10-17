import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, type App, type Component } from 'vue'
import HeaderLogo from '../HeaderLogo.vue'
import AnimatedLogo from '../AetherLineByLineLogo.vue'
import RippleLogo from '../RippleLogo.vue'
import { ARCA_LOGO_PATH, ARCA_LOGO_VIEWBOX, ARCA_REPOSITORY_URL } from '@/constants/arcaBrand'
import { AETHER_FULL_PATH, AETHER_SVG_VIEWBOX } from '@/constants/logoPaths'

const mounted: Array<{ app: App, root: HTMLElement }> = []

function mount(component: Component, props = {}) {
  vi.useFakeTimers()
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(component, props)
  app.mount(root)
  mounted.push({ app, root })
  return root
}

afterEach(() => {
  for (const { app, root } of mounted.splice(0)) {
    app.unmount()
    root.remove()
  }
  vi.useRealTimers()
})

describe('Arca brand assets', () => {
  it('points product GitHub icons to Arca without replacing upstream attribution', () => {
    expect(ARCA_REPOSITORY_URL).toBe('https://github.com/Becomingw/Arca')
    for (const path of ['src/layouts/MainLayout.vue', 'src/views/public/guide/GuideLayout.vue', 'src/views/public/Home.vue']) {
      const source = readFileSync(resolve(process.cwd(), path), 'utf8')
      expect(source).toContain(':href="ARCA_REPOSITORY_URL"')
    }
    const acknowledgements = readFileSync(resolve(process.cwd(), 'src/views/public/friend-links.ts'), 'utf8')
    expect(acknowledgements).toContain('https://github.com/fawney19/Aether')
  })
  it('uses the chosen arch in the shared header', () => {
    const root = mount(HeaderLogo)
    expect(root.querySelector('svg')?.getAttribute('viewBox')).toBe(ARCA_LOGO_VIEWBOX)
    expect(root.querySelector('path')?.getAttribute('d')).toBe(ARCA_LOGO_PATH)
  })

  it('keeps the favicon silhouette aligned with the inline mark', () => {
    const favicon = readFileSync(resolve(process.cwd(), 'public/arca.svg'), 'utf8')
    expect(favicon).toContain(`viewBox="${ARCA_LOGO_VIEWBOX}"`)
    expect(favicon).toContain(`d="${ARCA_LOGO_PATH}"`)
    expect(favicon).toContain('prefers-color-scheme:dark')
  })

  it('draws the Arca outline with the existing homepage animation', () => {
    const root = mount(AnimatedLogo, { brand: 'arca', autoStart: false })
    expect(root.querySelector('svg')?.getAttribute('viewBox')).toBe(ARCA_LOGO_VIEWBOX)
    const paths = [...root.querySelectorAll('path')]
    expect(paths.length).toBe(3)
    expect(paths.every(path => path.getAttribute('d') === ARCA_LOGO_PATH)).toBe(true)
  })

  it('preserves the legacy animation for other callers', () => {
    const root = mount(AnimatedLogo, { autoStart: false })
    expect(root.querySelector('svg')?.getAttribute('viewBox')).toBe(AETHER_SVG_VIEWBOX)
    expect([...root.querySelectorAll('path')].some(path => path.getAttribute('d') === AETHER_FULL_PATH)).toBe(true)
  })

  it('uses Arca in the feature section without replacing provider logos', () => {
    const root = mount(RippleLogo, { type: 'aether', brand: 'arca', static: true })
    expect(root.querySelector('svg')?.getAttribute('viewBox')).toBe(ARCA_LOGO_VIEWBOX)
    expect(root.querySelector('path')?.getAttribute('d')).toBe(ARCA_LOGO_PATH)
    const provider = mount(RippleLogo, { type: 'claude', brand: 'arca', static: true })
    expect(provider.querySelector('svg')?.getAttribute('viewBox')).toBe('0 0 24 24')
    expect(provider.querySelector('path')?.getAttribute('d')).not.toBe(ARCA_LOGO_PATH)
  })
})
