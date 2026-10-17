import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import Home from '../Home.vue'

const auth = vi.hoisted(() => ({ isAuthenticated: true, canAccessAdmin: true }))
vi.mock('@/stores/auth', () => ({ useAuthStore: () => auth }))
vi.mock('@/composables/useSiteInfo', () => ({ useSiteInfo: () => ({ siteName: ref('Arca'), siteSubtitle: ref('智枢 · AI Gateway') }) }))
vi.mock('@/components/common/ThemeModeButton.vue', () => ({ default: defineComponent({ setup: () => () => h('button', 'Theme') }) }))
vi.mock('@/components/common/LanguageSwitcher.vue', () => ({ default: defineComponent({ setup: () => () => h('button', 'Language') }) }))
vi.mock('../HarnessCarousel.vue', () => ({
  default: defineComponent({ props: { active: Boolean }, setup: props => () => h('div', { 'data-harness-active': String(props.active) }, 'Harness carousel') }),
}))
vi.mock('@/features/auth/components/LoginDialog.vue', () => ({
  default: defineComponent({ props: { modelValue: Boolean }, setup: props => () => props.modelValue ? h('div', { role: 'dialog' }, 'Login') : null }),
}))

const mounts: Array<{ app: App, root: HTMLElement }> = []
beforeEach(() => {
  auth.isAuthenticated = true
  auth.canAccessAdmin = true
})
afterEach(() => {
  for (const { app, root } of mounts.splice(0)) {
    app.unmount()
    root.remove()
  }
})

async function mount() {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(Home)
  const empty = { render: () => h('div') }
  const router = createRouter({ history: createMemoryHistory(), routes: ['/', '/guide', '/admin/dashboard', '/dashboard', '/admin/feedback', '/dashboard/feedback'].map(path => ({ path, component: empty })) })
  app.use(router)
  await router.push('/')
  await router.isReady()
  app.mount(root)
  mounts.push({ app, root })
  return root
}

describe('Arca homepage structure', () => {
  it('keeps five consecutive populated screens with no reserved screen', async () => {
    const root = await mount()
    expect(root.querySelector('h1')?.textContent?.trim()).toBe('Arca')
    expect(root.querySelector('.home-hero [data-harness-active]')).not.toBeNull()
    expect(root.querySelectorAll('main > section')).toHaveLength(5)
    expect(root.querySelector('#home-section-1 h2')?.textContent).toBe('多种协议，一个入口')
    expect(root.querySelector('#home-section-2 h2')?.textContent).toBe('让每个人拥有合适的权益')
    expect(root.querySelector('#home-section-3 h2')?.textContent).toBe('快捷反馈，Agent 协同运维')
    expect(root.querySelector('main > section:last-child h2')?.textContent).toBe('友情链接')
    expect(root.querySelectorAll('.home-reserved')).toHaveLength(0)
    for (const [index, section] of Array.from(root.querySelectorAll('main > section')).entries()) {
      expect(section.id).toBe(`home-section-${index}`)
      expect(section.querySelector('h1, h2')).not.toBeNull()
    }
    expect(root.textContent).not.toContain('功能开发进度')
    expect(root.textContent).not.toContain('config.toml')
    expect(root.querySelectorAll('.home-pagination button')).toHaveLength(5)
    expect(Array.from(root.querySelectorAll('.home-pagination button')).map(button => button.getAttribute('aria-label'))).toEqual(['首页', '接入', '权益', '运维', '友链'])
  })

  it.each([[true, '/admin/dashboard'], [false, '/dashboard']])('preserves the authenticated dashboard destination for admin=%s', async (admin, path) => {
    auth.canAccessAdmin = admin
    const root = await mount()
    expect(root.querySelector('.home-entry')?.getAttribute('href')).toBe(path)
    expect(root.querySelector('.home-header-nav a')?.getAttribute('href')).toBe('/guide')
    expect(root.querySelector('.operations-feedback-link')?.getAttribute('href')).toBe(admin ? '/admin/feedback' : '/dashboard/feedback')
  })

  it('opens the existing login dialog for unauthenticated visitors', async () => {
    auth.isAuthenticated = false
    const root = await mount()
    expect(root.querySelector('[role="dialog"]')).toBeNull()
    root.querySelector<HTMLButtonElement>('.home-entry')?.click()
    await nextTick()
    expect(root.querySelector('[role="dialog"]')).not.toBeNull()
  })

  it('uses the login dialog instead of a protected feedback link for guests', async () => {
    auth.isAuthenticated = false
    const root = await mount()
    expect(root.querySelector('.operations-feedback-link')?.tagName).toBe('BUTTON')
    root.querySelector<HTMLButtonElement>('.operations-feedback-link')?.click()
    await nextTick()
    expect(root.querySelector('[role="dialog"]')).not.toBeNull()
  })

  it('navigates to the protocol page without scrolling the outer document', async () => {
    const root = await mount()
    const container = root.querySelector<HTMLElement>('.arca-home')!
    const scrollTo = vi.fn()
    container.scrollTo = scrollTo
    Object.defineProperty(root.querySelector('.home-header'), 'offsetHeight', { value: 72 })
    Object.defineProperty(root.querySelector('#home-section-1'), 'offsetTop', { value: 772 })
    root.querySelector<HTMLButtonElement>('.home-next')?.click()
    expect(scrollTo).toHaveBeenCalledWith({ top: 700, behavior: 'smooth' })
  })

  it('updates the page indicator and stops the carousel on the feature pages', async () => {
    const root = await mount()
    const container = root.querySelector<HTMLElement>('.arca-home')!
    Object.defineProperty(container, 'clientHeight', { value: 700 })
    root.querySelectorAll('main > section').forEach((section, index) => {
      Object.defineProperty(section, 'offsetTop', { value: index * 700 })
    })
    container.scrollTop = 700
    container.dispatchEvent(new Event('scroll'))
    await nextTick()
    expect(root.querySelector('.home-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('接入')
    expect(root.querySelector('[data-harness-active]')?.getAttribute('data-harness-active')).toBe('false')
    expect(root.querySelector('.arca-protocol')?.getAttribute('data-active')).toBe('true')
    container.scrollTop = 0
    container.dispatchEvent(new Event('scroll'))
    await nextTick()
    expect(root.querySelector('[data-harness-active]')?.getAttribute('data-harness-active')).toBe('true')
    expect(root.querySelector('.arca-protocol')?.getAttribute('data-active')).toBe('false')
  })

  it('links benefits directly to operations, then friend links, and back to the hero', async () => {
    const root = await mount()
    const container = root.querySelector<HTMLElement>('.arca-home')!
    const scrollTo = vi.fn()
    container.scrollTo = scrollTo
    Object.defineProperty(root.querySelector('.home-header'), 'offsetHeight', { value: 72 })
    Object.defineProperty(root.querySelector('#home-section-3'), 'offsetTop', { value: 2172 })
    Object.defineProperty(root.querySelector('#home-section-4'), 'offsetTop', { value: 2872 })
    Object.defineProperty(root.querySelector('#home-section-0'), 'offsetTop', { value: 72 })
    root.querySelector<HTMLButtonElement>('#home-section-2 .home-next')?.click()
    expect(scrollTo).toHaveBeenLastCalledWith({ top: 2100, behavior: 'smooth' })
    root.querySelector<HTMLButtonElement>('#home-section-3 .home-next')?.click()
    expect(scrollTo).toHaveBeenLastCalledWith({ top: 2800, behavior: 'smooth' })
    root.querySelector<HTMLButtonElement>('.friend-return')?.click()
    expect(scrollTo).toHaveBeenLastCalledWith({ top: 0, behavior: 'smooth' })
  })
})
