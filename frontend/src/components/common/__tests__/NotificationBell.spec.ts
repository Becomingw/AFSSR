import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, reactive, type App } from 'vue'
import NotificationBell from '../NotificationBell.vue'
import { resetNotificationsForTests } from '@/composables/useNotifications'

const mocks = vi.hoisted(() => ({ list: vi.fn(), mark: vi.fn(), push: vi.fn(), auth: { user: { id: 'u1', role: 'user' } } }))
vi.mock('@/api/feedback', () => ({ listNotifications: mocks.list, markNotificationsRead: mocks.mark }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push: mocks.push }) }))
vi.mock('@/stores/auth', async () => {
  const { reactive } = await import('vue')
  mocks.auth = reactive(mocks.auth)
  return { useAuthStore: () => mocks.auth }
})

const note = (id: number, read = false) => ({
  id, kind: 'mention', issue_id: `issue-${id}`, issue_number: 10 + id, issue_title: `Topic ${id}`, event_seq: id,
  actor_username: 'alice', snippet: `ping ${id}`, created_at: Math.floor(Date.now() / 1000) - 120, read_at: read ? 1 : null,
})
let app: App
let root: HTMLDivElement
async function flush() { for (let i = 0; i < 10; i++) { await Promise.resolve(); await nextTick() } }
const bell = () => root.querySelector<HTMLButtonElement>('.bell-button')!

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] })
  vi.clearAllMocks()
  resetNotificationsForTests()
  mocks.auth.user = reactive({ id: 'u1', role: 'user' })
  mocks.list.mockResolvedValue({ items: [note(1), note(2, true)], unread_count: 1, next_before: null })
  mocks.mark.mockResolvedValue(0)
  root = document.createElement('div')
  document.body.append(root)
  app = createApp(NotificationBell)
  app.mount(root)
  await flush()
})
afterEach(() => { app.unmount(); root.remove(); vi.useRealTimers() })

describe('NotificationBell', () => {
  it('shows the unread count from a cheap poll, without loading the list', async () => {
    expect(mocks.list).toHaveBeenCalledWith({ unread: true, limit: 1 })
    expect(root.querySelector('[data-testid="notification-badge"]')!.textContent).toBe('1')
    expect(bell().getAttribute('aria-label')).toContain('1 条未读')
    expect(root.querySelector('.bell-panel')).toBeNull()
  })

  it('lists mentions when opened and reads an item by opening its topic', async () => {
    bell().click()
    await flush()
    const items = root.querySelectorAll('.bell-item')
    expect(items).toHaveLength(2)
    expect(items[0]!.textContent).toContain('alice')
    expect(items[0]!.textContent).toContain('#11 Topic 1')
    expect(items[0]!.textContent).toContain('ping 1')
    expect(items[0]!.classList.contains('unread')).toBe(true)
    expect(items[1]!.classList.contains('unread')).toBe(false)
    ;(items[0] as HTMLButtonElement).click()
    await flush()
    expect(mocks.mark).toHaveBeenCalledWith({ ids: [1] })
    expect(mocks.push).toHaveBeenCalledWith('/dashboard/feedback/11')
    expect(root.querySelector('.bell-panel')).toBeNull()
    expect(root.querySelector('[data-testid="notification-badge"]')).toBeNull()
  })

  it('sends administrators to the console copy of the topic', async () => {
    mocks.auth.user.role = 'admin'
    bell().click()
    await flush()
    ;(root.querySelector('.bell-item') as HTMLButtonElement).click()
    await flush()
    expect(mocks.push).toHaveBeenCalledWith('/admin/feedback/11')
  })

  it('marks everything read in one request', async () => {
    bell().click()
    await flush()
    const all = [...root.querySelectorAll('button')].find(b => b.textContent?.includes('全部标为已读'))!
    all.click()
    await flush()
    expect(mocks.mark).toHaveBeenCalledWith({ all: true })
    expect(root.querySelectorAll('.bell-item.unread')).toHaveLength(0)
    expect(root.querySelector('[data-testid="notification-badge"]')).toBeNull()
  })

  it('closes with Escape or a click elsewhere, and explains an empty or failed list', async () => {
    mocks.list.mockResolvedValue({ items: [], unread_count: 0, next_before: null })
    bell().click()
    await flush()
    expect(root.textContent).toContain('还没有人提到你')
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flush()
    expect(root.querySelector('.bell-panel')).toBeNull()
    mocks.list.mockRejectedValue(new Error('down'))
    bell().click()
    await flush()
    expect(root.querySelector('[role="alert"]')!.textContent).toContain('暂时无法加载')
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
    await flush()
    expect(root.querySelector('.bell-panel')).toBeNull()
  })

  it('polls again on an interval and keeps the old count when a poll fails', async () => {
    mocks.list.mockClear()
    mocks.list.mockResolvedValue({ items: [], unread_count: 3, next_before: null })
    vi.advanceTimersByTime(60_000)
    await flush()
    expect(mocks.list).toHaveBeenCalledTimes(1)
    expect(root.querySelector('[data-testid="notification-badge"]')!.textContent).toBe('3')
    mocks.list.mockRejectedValue(new Error('offline'))
    vi.advanceTimersByTime(60_000)
    await flush()
    expect(root.querySelector('[data-testid="notification-badge"]')!.textContent).toBe('3')
  })
})
