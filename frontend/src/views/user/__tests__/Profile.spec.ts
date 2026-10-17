import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import Profile from '../Profile.vue'

const mocks = vi.hoisted(() => ({ profile: vi.fn(), usage: vi.fn(), models: vi.fn(), quotas: vi.fn(), auth: { user: { id: 'user-1', username: 'research-user', avatar_version: null as number | null } } }))
vi.mock('@/stores/auth', () => ({ useAuthStore: () => mocks.auth }))
vi.mock('@/api/me', () => ({ meApi: { getProfile: mocks.profile, getUsage: mocks.usage, getAvailableModels: mocks.models } }))
vi.mock('../profile/TokenActivity.vue', () => ({ default: defineComponent({ setup() { return () => h('section', { 'data-token-activity': '' }, 'Token 活动') } }) }))
vi.mock('@/api/account', () => ({ accountApi: { entitlements: mocks.quotas } }))
vi.mock('../Settings.vue', () => ({ default: defineComponent({ props: { embedded: Boolean, section: String }, setup(props) { return () => h('div', { 'data-settings-section': props.section }, `settings-${props.section}`) } }) }))
vi.mock('vue-router', () => ({ RouterLink: defineComponent({ props: { to: String }, setup(props, { slots }) { return () => h('a', { href: props.to }, slots.default?.()) } }) }))

let app: App, root: HTMLDivElement
const quota = { enabled: true, generated_at: 1792152000, timezone: 'Asia/Shanghai', source: 'personal', unlimited: false, windows: [
  { model: null, period: 'five_hour', used_usd: 3, reserved_usd: 1, limit_usd: 5, remaining_usd: 1, starts_at: 1, ends_at: 2, resets_at: null },
  { model: null, period: 'weekly', used_usd: 3, reserved_usd: 1, limit_usd: 3, remaining_usd: 0, starts_at: 1, ends_at: 2, resets_at: 1792339200 },
  { model: null, period: 'monthly', used_usd: 3, reserved_usd: 1, limit_usd: null, remaining_usd: null, starts_at: 1, ends_at: 2, resets_at: 1793462400 },
  { model: 'fabel', period: 'daily', used_usd: 2, reserved_usd: 0, limit_usd: 5, remaining_usd: 3, starts_at: 1, ends_at: 2, resets_at: 1792252800 },
] }
async function flush() { for (let i = 0; i < 12; i++) { await Promise.resolve(); await nextTick() } }
function click(label: string) { const button = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === label); expect(button).toBeTruthy(); button?.click() }
beforeEach(async () => {
  vi.clearAllMocks()
  mocks.auth.user.avatar_version = null
  mocks.profile.mockResolvedValue({ username: 'research-user', email: 'self@example.test', is_active: true, role: 'user', created_at: '2026-10-15T00:00:00Z' })
  mocks.quotas.mockResolvedValue(structuredClone(quota))
  mocks.usage.mockResolvedValue({ total_requests: 42, total_tokens: 125000, total_cost: 3.5, summary_by_model: [{ model: 'fabel', requests: 42, total_tokens: 125000, total_cost_usd: 3.5 }], records: [] })
  mocks.models.mockResolvedValue({ total: 1, models: [{ id: 'fabel', name: 'fabel' }] })
  root = document.createElement('div'); document.body.append(root); app = createApp(Profile); app.mount(root); await flush()
})
afterEach(() => { app.unmount(); root.remove() })

describe('personal profile', () => {
  it('shows the uploaded picture when there is one and the initial otherwise', async () => {
    const avatar = () => root.querySelector('.profile-heading .user-avatar')!
    expect(avatar().querySelector('img')).toBeNull()
    expect(avatar().textContent?.trim()).toBe('R')
    app.unmount(); root.remove()
    mocks.auth.user.avatar_version = 1792300000000
    root = document.createElement('div'); document.body.append(root); app = createApp(Profile); app.mount(root); await flush()
    expect(avatar().querySelector('img')?.getAttribute('src')).toBe('/api/avatars/user-1?v=1792300000000')
  })
  it('keeps family usage out of total windows and groups its shared members once', async () => {
    mocks.quotas.mockResolvedValue({ ...quota, windows: [...quota.windows,
      { ...quota.windows[0], model: null, family: 'f1', family_name: 'Fabel 家族', models: ['fabel', 'fabel-pro'], limit_usd: 2 },
      { ...quota.windows[1], model: null, family: 'f1', family_name: 'Fabel 家族', models: ['fabel', 'fabel-pro'], limit_usd: 10 },
    ] })
    root.querySelector<HTMLButtonElement>('[aria-label="刷新额度"]')!.click(); await flush()
    expect(root.querySelector('.quota-list')?.querySelectorAll('[role="progressbar"]')).toHaveLength(2)
    const pools = root.querySelectorAll('.model-pool')
    expect(pools).toHaveLength(2)
    expect([...pools].find(pool => pool.textContent?.includes('Fabel 家族'))?.textContent).toContain('fabel、fabel-pro')
    expect([...pools].find(pool => pool.textContent?.includes('Fabel 家族'))?.textContent).toContain('2 项限额')
  })
  it('uses only self-scoped APIs and shows real reserved usage without exposing wallet features', () => {
    expect(mocks.quotas).toHaveBeenCalledWith(expect.any(AbortSignal))
    expect(mocks.usage).toHaveBeenCalledWith({ preset: 'this_month', tz_offset_minutes: expect.any(Number), limit: 5, offset: 0 })
    expect(root.textContent).toContain('个人专属配置')
    expect(root.textContent).toContain('处理中 $1.00')
    expect(root.textContent).toContain('125,000')
    expect(root.textContent).toContain('子池用量同时计入总额度')
    expect(root.textContent).not.toContain('钱包')
    expect(root.querySelector('[data-token-activity]')).toBeTruthy()
    const bars = root.querySelectorAll('[role="progressbar"]')
    expect(bars[0]?.getAttribute('aria-valuenow')).toBe('80')
    expect(bars[1]?.getAttribute('aria-valuenow')).toBe('100')
    expect(root.textContent).toContain('无上限')
    expect(root.textContent).toContain('滚动窗口，用量随时间逐步释放')
    expect(root.textContent).toContain('Asia/Shanghai')
  })
  it('keeps existing account, security and preference controls reachable', async () => {
    for (const [label, section] of [['个人信息', 'account'], ['安全', 'security'], ['偏好设置', 'preferences']]) {
      click(label!); await flush(); expect(root.querySelector(`[data-settings-section="${section}"]`)).toBeTruthy()
      expect(root.querySelector('nav [aria-current="page"]')?.textContent).toContain(label)
    }
    click('用量与权益'); await flush(); expect(root.querySelector('[aria-labelledby="entitlement-title"]')).toBeTruthy()
  })
  it('isolates quota and usage errors and can retry without rendering fake zero budgets', async () => {
    mocks.quotas.mockRejectedValue(new Error('offline'))
    root.querySelector<HTMLButtonElement>('[aria-label="刷新额度"]')!.click(); await flush()
    expect(root.textContent).toContain('额度暂不可用，不代表没有额度')
    expect(root.querySelector('.quota-list')).toBeNull()
    expect(root.textContent).toContain('125,000')
    mocks.quotas.mockResolvedValue(quota); click('重试'); await flush()
    expect(root.querySelector('.quota-list')).toBeTruthy()
    mocks.usage.mockRejectedValue(new Error('offline'))
    root.querySelector<HTMLButtonElement>('[aria-label="刷新调用统计"]')!.click(); await flush()
    expect(root.querySelector('.usage-metrics')).toBeNull()
    expect(root.querySelector('.quota-list')).toBeTruthy()
  })
  it('does not let an older interval response replace a new selection', async () => {
    let resolveOld!: (value: unknown) => void
    mocks.usage.mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve }))
    const select = root.querySelector<HTMLSelectElement>('[aria-label="个人用量统计区间"]')!
    select.value = 'today'; select.dispatchEvent(new Event('change')); await flush()
    mocks.usage.mockResolvedValue({ total_requests: 7, total_tokens: 777, total_cost: 0, summary_by_model: [], records: [] })
    select.value = 'last7days'; select.dispatchEvent(new Event('change')); await flush()
    resolveOld({ total_requests: 9, total_tokens: 999999, total_cost: 0, summary_by_model: [], records: [] }); await flush()
    expect(root.querySelector('.usage-metrics')?.textContent).toContain('777')
    expect(root.textContent).not.toContain('999,999')
  })
})
