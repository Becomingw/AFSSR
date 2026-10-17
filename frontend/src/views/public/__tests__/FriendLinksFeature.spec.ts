import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'
import { setI18nLocale } from '@/i18n'
import FriendLinksFeature from '../FriendLinksFeature.vue'

const mounts: Array<{ app: App, root: HTMLElement }> = []
function mount(onHome = vi.fn()) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(FriendLinksFeature, { onHome })
  app.mount(root)
  mounts.push({ app, root })
  return root
}
afterEach(() => {
  for (const { app, root } of mounts.splice(0)) { app.unmount(); root.remove() }
  vi.unstubAllGlobals()
})

const destinations = [
  'https://ai4rad-ssph.github.io/',
  'https://github.com/FrankZhangRp/Acute_abdomen',
  'https://frankzhangrp.github.io/BoneCoT/',
  'https://github.com/Becomingw/CGP-Plaque',
  'https://github.com/Becomingw/RescueCTA',
  'https://github.com/Becomingw/Margin',
  'https://github.com/fawney19/Aether',
  'https://github.com/Becomingw/rmount-ctx',
]

describe('homepage friend links', () => {
  it('includes each of the eight supplied destinations exactly once with safe external navigation', () => {
    const root = mount()
    const links = Array.from(root.querySelectorAll<HTMLAnchorElement>('a'))
    expect(links).toHaveLength(8)
    expect(links.map(link => link.href).sort()).toEqual([...destinations].sort())
    for (const link of links) {
      expect(link.target).toBe('_blank')
      expect(link.relList.contains('noopener')).toBe(true)
      expect(link.relList.contains('noreferrer')).toBe(true)
      expect(link.getAttribute('aria-label')).toContain('在新标签页打开')
    }
  })

  it('gives the lab a dedicated row and distinguishes research from open source and upstream thanks', () => {
    const root = mount()
    expect(root.querySelector('.friend-institution')?.textContent).toContain('AI4RadLab')
    expect(root.querySelector('.friend-institution')?.textContent).toContain('上海交通大学医学院 × 上海市第六人民医院')
    expect(root.querySelectorAll('.friend-group')).toHaveLength(2)
    expect(root.querySelectorAll('.friend-group:first-child a')).toHaveLength(4)
    expect(root.querySelectorAll('.friend-group:last-child a')).toHaveLength(3)
    expect(root.querySelectorAll('.friend-acknowledgement')).toHaveLength(1)
    expect(root.querySelector('.friend-acknowledgement')?.textContent).toContain('Aether')
    expect(root.querySelector('.friend-acknowledgement')?.textContent).toContain('感谢原项目作者与开源社区')
  })

  it('uses the official project spellings and does not claim institutional endorsement', () => {
    const root = mount()
    for (const name of ['AbdomenNet', 'BoneCoT', 'CGP-Plaque', 'RescueCTA', 'Margin. 页边', 'rmount-ctx']) {
      expect(root.textContent).toContain(name)
    }
    expect(root.textContent).not.toContain('合作伙伴')
    expect(root.textContent).not.toContain('官方背书')
  })

  it('renders the lab adaptation as a scalable themed SVG without bitmap or font dependencies', () => {
    const root = mount()
    const mark = root.querySelector('.friend-institution svg.friend-institution-icon')
    expect(mark?.getAttribute('viewBox')).toBe('0 0 100 100')
    expect(mark?.querySelectorAll('path').length).toBeGreaterThan(4)
    expect(mark?.querySelector('image, foreignObject, text, script')).toBeNull()
    expect(mark?.innerHTML).toContain('--ai4rad-teal')
    expect(mark?.innerHTML).toContain('--ai4rad-accent')
    expect(root.querySelector('.friend-institution .lucide-landmark')).toBeNull()
  })

  it('returns to the hero without requesting remote metadata', () => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)
    const onHome = vi.fn()
    const root = mount(onHome)
    root.querySelector<HTMLButtonElement>('.friend-return')?.click()
    expect(onHome).toHaveBeenCalledOnce()
    expect(fetch).not.toHaveBeenCalled()
  })

  it('localizes descriptions and navigation while preserving names and destinations', async () => {
    setI18nLocale('en-US')
    const root = mount()
    await nextTick()
    expect(root.querySelector('h2')?.textContent).toContain('Friends & Open Source')
    expect(root.querySelector('.friend-acknowledgement')?.textContent).toContain('original authors')
    expect(root.querySelector('.friend-return')?.getAttribute('aria-label')).toBe('Back to home')
    expect(root.querySelector('.friend-institution')?.textContent).toContain('Shanghai Jiao Tong University School of Medicine')
    expect(root.textContent).not.toContain('site.home.links.')
    expect(root.querySelectorAll('a')).toHaveLength(8)
  })
})
