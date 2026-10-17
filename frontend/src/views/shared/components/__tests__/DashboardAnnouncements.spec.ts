import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, type App } from 'vue'
import type { Announcement } from '@/api/announcements'
import DashboardAnnouncements from '../DashboardAnnouncements.vue'

vi.mock('vue-router', async () => {
  const { defineComponent, h } = await import('vue')
  return { RouterLink: defineComponent({
    props: { to: { type: String, required: true } },
    setup(props, { slots }) { return () => h('a', { href: props.to }, slots.default?.()) },
  }) }
})

let app: App | undefined
let root: HTMLDivElement | undefined
afterEach(() => { app?.unmount(); root?.remove() })

function announcement(id: string, extra: Partial<Announcement> = {}): Announcement {
  return { id, title: `Notice ${id}`, content: '**Important** [details](https://example.invalid)', type: 'info', priority: 0, is_pinned: false, is_active: true, requires_ack: false, author: { id: 'author', username: 'Admin' }, created_at: '2026-10-16T00:00:00Z', updated_at: '2026-10-16T00:00:00Z', is_read: false, ...extra }
}

function render(items: Announcement[] = [], extra = {}) {
  const select = vi.fn(), refresh = vi.fn()
  root = document.createElement('div'); document.body.append(root)
  app = createApp({ render: () => h(DashboardAnnouncements, { items, loading: false, error: false, allUrl: '/dashboard/announcements', onSelect: select, onRefresh: refresh, ...extra }) })
  app.mount(root)
  return { root, select, refresh }
}

describe('dashboard announcement spotlight', () => {
  it('features pinned notices before priority, without mutating server order', () => {
    const items = [announcement('priority', { priority: 100 }), announcement('pinned', { is_pinned: true }), announcement('older', { created_at: '2026-10-14T00:00:00Z' })]
    const { root } = render(items)
    expect(root.querySelector('.announcement-featured')?.getAttribute('aria-label')).toBe('查看公告：Notice pinned')
    expect(items.map(item => item.id)).toEqual(['priority', 'pinned', 'older'])
    expect(root.textContent).toContain('3 条未读')
    expect(root.querySelectorAll('.announcement-recent__item')).toHaveLength(2)
  })

  it('uses priority then publication date for unpinned notices', () => {
    const { root } = render([announcement('old', { priority: 3 }), announcement('low'), announcement('new', { priority: 3, created_at: '2026-10-17T00:00:00Z' })])
    expect(root.querySelector('.announcement-featured')?.getAttribute('aria-label')).toBe('查看公告：Notice new')
  })

  it('opens the exact selected item and keeps read/ack semantics in the parent', () => {
    const item = announcement('pinned', { requires_ack: true, is_pinned: true, is_read: true })
    const { root, select } = render([item])
    root.querySelector<HTMLButtonElement>('.announcement-featured')!.click()
    expect(select).toHaveBeenCalledWith(item)
    expect(root.textContent).toContain('需确认已读')
    expect(root.textContent).toContain('已读')
    expect(root.textContent).not.toContain('条未读')
    expect(root.querySelector('.announcement-recent')).toBeNull()
  })

  it('shows a plain-text summary and bounded secondary list with a full-list link', () => {
    const { root } = render(Array.from({ length: 8 }, (_, i) => announcement(String(i))))
    expect(root.querySelector('.announcement-featured__excerpt')?.textContent).toBe('Important details')
    expect(root.querySelector('.announcement-featured__excerpt')?.querySelector('a')).toBeNull()
    expect(root.querySelectorAll('.announcement-recent__item')).toHaveLength(3)
    expect(root.querySelector('a')?.getAttribute('href')).toBe('/dashboard/announcements')
  })

  it('distinguishes failure from an empty list and supports retry', () => {
    const { root, refresh } = render([], { error: true })
    expect(root.textContent).toContain('公告暂时无法加载')
    expect(root.textContent).not.toContain('暂无公告')
    root.querySelector<HTMLButtonElement>('button')!.click()
    expect(refresh).toHaveBeenCalledOnce()
  })

  it('renders a compact empty state', () => {
    const { root } = render()
    expect(root.textContent).toContain('暂无公告')
    expect(root.querySelector('.announcement-featured')).toBeNull()
  })

  it('does not flash an old notice while loading', () => {
    const { root } = render([announcement('old')], { loading: true })
    expect(root.querySelector('section')?.getAttribute('aria-busy')).toBe('true')
    expect(root.textContent).toContain('公告加载中')
    expect(root.querySelector('.announcement-featured')).toBeNull()
  })
})
