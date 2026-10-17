import { afterEach, describe, expect, it } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'

import SidebarNav from '@/components/layout/SidebarNav.vue'

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

const TestIcon = defineComponent({
  name: 'TestIcon',
  setup: () => () => h('svg', { 'data-testid': 'nav-icon' }),
})

const RouterLinkStub = defineComponent({
  name: 'RouterLink',
  inheritAttrs: false,
  props: {
    to: {
      type: String,
      required: true,
    },
  },
  setup(props, { attrs, slots }) {
    return () => h('a', { ...attrs, href: props.to }, slots.default?.())
  },
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
})

describe('SidebarNav', () => {
  it('switches from labelled navigation to accessible icon-only links', async () => {
    const collapsed = ref(false)
    const root = document.createElement('div')
    document.body.appendChild(root)

    const app = createApp({
      setup: () => () => h(SidebarNav, {
        items: [{
          title: 'Overview',
          items: [
            { name: 'Dashboard', href: '/dashboard', icon: TestIcon },
            { name: 'Usage', href: '/usage', icon: TestIcon },
          ],
        }, {
          title: 'Management',
          items: [
            { name: 'Settings', href: '/settings', icon: TestIcon },
          ],
        }],
        activePath: '/dashboard',
        collapsed: collapsed.value,
      }),
    })
    app.component('RouterLink', RouterLinkStub)
    app.mount(root)
    mountedApps.push({ app, root })

    expect(root.querySelector('nav')?.dataset.collapsed).toBe('false')
    expect(root.textContent).toContain('Overview')
    expect(root.textContent).toContain('Management')
    expect(root.textContent).toContain('Dashboard')
    expect(root.querySelectorAll('[data-sidebar-group-divider]')).toHaveLength(0)
    const expandedDashboardLink = root.querySelector('a[href="/dashboard"]')
    expect(expandedDashboardLink?.getAttribute('aria-current')).toBe('page')
    expect(root.querySelector('a[href="/usage"]')?.hasAttribute('aria-current')).toBe(false)
    expect(expandedDashboardLink?.hasAttribute('aria-label')).toBe(false)
    expect(expandedDashboardLink?.classList.contains('py-2')).toBe(true)
    expect(expandedDashboardLink?.classList.contains('h-9')).toBe(false)

    collapsed.value = true
    await nextTick()

    expect(root.querySelector('nav')?.dataset.collapsed).toBe('true')
    expect(root.textContent).not.toContain('Overview')
    expect(root.textContent).not.toContain('Management')
    expect(root.textContent).not.toContain('Dashboard')
    expect(root.querySelectorAll('[data-testid="nav-icon"]')).toHaveLength(3)
    expect(root.querySelectorAll('[data-sidebar-group-divider]')).toHaveLength(1)
    const collapsedDashboardLink = root.querySelector('a[href="/dashboard"]')
    expect(collapsedDashboardLink?.getAttribute('aria-label')).toBe('Dashboard')
    expect(collapsedDashboardLink?.classList.contains('h-9')).toBe(true)
    expect(collapsedDashboardLink?.classList.contains('py-2')).toBe(false)
    expect(root.querySelector('a[href="/usage"]')?.getAttribute('aria-label')).toBe('Usage')
  })
})
