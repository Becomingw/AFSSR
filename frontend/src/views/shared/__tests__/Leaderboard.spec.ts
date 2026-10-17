import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import Leaderboard from '../Leaderboard.vue'
import { canAccessAdminPage } from '@/config/adminRoles'

const mocks = vi.hoisted(() => ({ get: vi.fn(), route: { path: '/dashboard/leaderboard' } }))
vi.mock('vue-router', async () => { const { reactive } = await import('vue'); mocks.route = reactive(mocks.route); return { useRoute: () => mocks.route } })
vi.mock('@/api/leaderboard', () => ({ getLeaderboard: mocks.get }))
vi.mock('@/components/ui', () => ({ Button: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }) }))
let app: App, root: HTMLDivElement
const entry = { rank: 1, id: 'member-1', name: '成员 001', value: 10 }
const fixture = { masked: true, users: 3, models: 1, requests: 30, tokens: 900, cost: null, start_date: '2026-10-12', end_date: '2026-10-16', generated_at: 1700000000, tz_offset_minutes: 480, activity_observed_from: 1,
  boards: ['models', 'requests', 'tokens', 'cost', 'daily_active', 'weekly_active', 'active_days', 'streak', 'early_wins', 'night_active', 'night_wins'].map(key => ({ key, unit: key === 'cost' ? 'USD' : key === 'night_active' ? '分钟' : '次', items: [{ ...entry, value: key === 'cost' ? null : 10 }] })),
  hours: Array.from({ length: 24 }, (_, hour) => ({ hour, requests: 3, leaders: [entry] })), early_birds: [{ date: '2026-10-16', winners: [entry], time: null }], night_owls: [{ date: '2026-10-15', winners: [{ ...entry, value: 60 }] }] }
async function flush() { for (let i = 0; i < 12; i++) { await Promise.resolve(); await nextTick() } }
function click(label: string) { const button = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === label); expect(button).toBeTruthy(); button?.click() }
beforeEach(async () => {
  vi.clearAllMocks(); mocks.route.path = '/dashboard/leaderboard'; mocks.get.mockResolvedValue(structuredClone(fixture))
  root = document.createElement('div'); document.body.append(root); app = createApp(Leaderboard); app.mount(root); await flush()
})
afterEach(() => { app.unmount(); root.remove() })
describe('leaderboard views', () => {
  it('loads the masked report for users and renders user-controlled labels as text', async () => {
    expect(mocks.get).toHaveBeenCalledWith(false, 'this_week', 480, expect.any(AbortSignal))
    expect(root.textContent).toContain('脱敏视图')
    const poisoned = structuredClone(fixture); poisoned.boards[0]!.items[0]!.name = '<img src=x onerror=alert(1)>'
    mocks.get.mockResolvedValue(poisoned); root.querySelector<HTMLButtonElement>('[title="刷新排行榜"]')?.click(); await flush()
    expect(root.querySelector('img')).toBeNull(); expect(root.textContent).toContain('<img src=x onerror=alert(1)>')
  })
  it('does not reveal amounts in the public cost ranking', async () => {
    const select = root.querySelector<HTMLSelectElement>('[aria-label="用户排名指标"]')!
    select.value = 'cost'; select.dispatchEvent(new Event('change')); await flush()
    expect(root.textContent).toContain('未公开'); expect(root.textContent).not.toContain('$')
  })
  it('shows activity methodology, hourly leaders and early birds without precise public times', async () => {
    click('活跃排行'); await flush(); expect(root.textContent).toContain('并非实际工时')
    expect(root.textContent).toContain('自然周活跃时长')
    click('时段与荣誉'); await flush()
    expect(root.querySelectorAll('[aria-label*="时，"]')).toHaveLength(24)
    expect(root.textContent).toContain('每日早鸟之王'); expect(root.textContent).not.toContain('private')
    expect(root.textContent).toContain('每天 06:00 起')
    expect(root.textContent).toContain('熬夜之王')
    expect(root.textContent).toContain('22:00 至次日 06:00')
    expect(root.textContent).toContain('2026-10-15')
    expect(root.textContent).toContain('60 分钟')
  })
  it('reloads when the range/timezone or administrative context changes', async () => {
    for (const role of ['admin', 'audit_admin', 'user_admin', 'service_admin']) expect(canAccessAdminPage(role, '/admin/leaderboard')).toBe(true)
    mocks.route.path = '/admin/leaderboard'; await flush()
    expect(mocks.get).toHaveBeenLastCalledWith(true, 'this_week', 480, expect.any(AbortSignal))
    const select = root.querySelector<HTMLSelectElement>('[aria-label="统计区间"]')!
    select.value = 'last_week'; select.dispatchEvent(new Event('change')); await flush()
    expect(mocks.get).toHaveBeenLastCalledWith(true, 'last_week', 480, expect.any(AbortSignal))
  })
  it('shows a recoverable error instead of presenting missing data as zeros', async () => {
    mocks.get.mockRejectedValue(new Error('offline')); root.querySelector<HTMLButtonElement>('[title="刷新排行榜"]')?.click(); await flush()
    expect(root.querySelector('[role="alert"]')).toBeTruthy(); expect(root.textContent).not.toContain('调用总次数')
    mocks.get.mockResolvedValue(fixture); click('重试'); await flush(); expect(root.querySelector('[role="alert"]')).toBeNull()
  })
})
