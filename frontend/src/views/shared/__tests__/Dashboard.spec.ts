import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import { createMemoryHistory, createRouter } from 'vue-router'

import Dashboard from '../Dashboard.vue'

const dashboardApiMocks = vi.hoisted(() => ({
  getStats: vi.fn(),
  getDailyStats: vi.fn(),
}))
const announcementMocks = vi.hoisted(() => ({ getAnnouncements: vi.fn(), markAsRead: vi.fn() }))

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => ({
    canAccessAdmin: false,
    isAdmin: false,
    isAuditAdmin: false,
  }),
}))

vi.mock('@/api/dashboard', () => ({
  dashboardApi: dashboardApiMocks,
}))

vi.mock('@/api/announcements', () => ({
  announcementApi: announcementMocks,
}))

vi.mock('@/components/charts/BarChart.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return { default: defineComponent({ name: 'BarChartStub', setup: () => () => h('div') }) }
})

vi.mock('@/components/charts/DoughnutChart.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return { default: defineComponent({ name: 'DoughnutChartStub', setup: () => () => h('div') }) }
})

vi.mock('@/components/charts/LineChart.vue', async () => {
  const { defineComponent, h } = await import('vue')
  return { default: defineComponent({ name: 'LineChartStub', setup: () => () => h('div') }) }
})

vi.mock('@/components/common', async () => {
  const { defineComponent, h } = await import('vue')
  return {
    TimeRangePicker: defineComponent({
      name: 'TimeRangePickerStub',
      setup() {
        return () => h('div')
      },
    }),
  }
})

vi.mock('@/components/ui', async () => {
  const { defineComponent, h } = await import('vue')
  const passthrough = (name: string, tag = 'div') => defineComponent({
    name,
    setup(_, { slots }) {
      return () => h(tag, slots.default?.())
    },
  })
  return {
    Card: passthrough('CardStub', 'section'),
    Badge: passthrough('BadgeStub', 'span'),
    Button: passthrough('ButtonStub', 'button'),
    Skeleton: defineComponent({ name: 'SkeletonStub', setup: () => () => h('div') }),
    Dialog: passthrough('DialogStub'),
    Table: passthrough('TableStub', 'table'),
    TableHeader: passthrough('TableHeaderStub', 'thead'),
    TableBody: passthrough('TableBodyStub', 'tbody'),
    TableRow: passthrough('TableRowStub', 'tr'),
    TableHead: passthrough('TableHeadStub', 'th'),
    TableCell: passthrough('TableCellStub', 'td'),
  }
})

vi.mock('lucide-vue-next', async () => {
  const Icon = defineComponent({
    name: 'IconStub',
    setup() {
      return () => h('span')
    },
  })
  return {
    Users: Icon,
    Activity: Icon,
    TrendingUp: Icon,
    DollarSign: Icon,
    Key: Icon,
    Hash: Icon,
    Zap: Icon,
    Bell: Icon,
    ArrowRight: Icon,
    Pin: Icon,
    AlertCircle: Icon,
    AlertTriangle: Icon,
    Info: Icon,
    Wrench: Icon,
    Loader2: Icon,
    Clock: Icon,
    Database: Icon,
    Shuffle: Icon,
  }
})

const mountedApps: Array<{ app: App, root: HTMLElement }> = []

function mountDashboard() {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(Dashboard)
  app.use(createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: { render: () => h('div') } }, { path: '/dashboard/announcements', component: { render: () => h('div') } }] }))
  app.mount(root)
  mountedApps.push({ app, root })
  return root
}

async function settle() {
  for (let index = 0; index < 8; index += 1) {
    await Promise.resolve()
    await nextTick()
  }
}

beforeEach(() => {
  announcementMocks.getAnnouncements.mockReset().mockResolvedValue({ items: [] })
  announcementMocks.markAsRead.mockReset().mockResolvedValue({})
  dashboardApiMocks.getStats.mockReset()
  dashboardApiMocks.getDailyStats.mockReset()
  dashboardApiMocks.getDailyStats.mockResolvedValue({
    daily_stats: [],
    model_summary: [],
    period: { start_date: '2026-05-01', end_date: '2026-05-15', days: 15 },
  })
})

afterEach(() => {
  for (const { app, root } of mountedApps.splice(0)) {
    app.unmount()
    root.remove()
  }
  document.body.innerHTML = ''
})

describe('Dashboard ordinary user wallet card', () => {
  it('renders package and wallet balance split from mocked stats', async () => {
    dashboardApiMocks.getStats.mockResolvedValue({
      stats: [
        { name: 'API 密钥', value: '0', subValue: '活跃 0', icon: 'Activity' },
        { name: '本月请求', value: '0', subValue: '今日 0', icon: 'Users' },
        {
          name: '钱包余额',
          value: '$110.00',
          subValue: '套餐额度 $100.00 · 钱包余额 $10.00',
          icon: 'DollarSign',
        },
        { name: '本月 Token', value: '0', subValue: '输入 0 / 输出 0', icon: 'Zap' },
      ],
      today: { requests: 0, tokens: 0, cost: 0 },
      cache_stats: { cache_creation_tokens: 0, cache_read_tokens: 0, total_cache_tokens: 0 },
      token_breakdown: { input: 0, output: 0, cache_creation: 0, cache_read: 0 },
      monthly_cost: 0,
    })

    const root = mountDashboard()
    await settle()

    expect(root.textContent).toContain('$110.00')
    expect(root.textContent).toContain('套餐额度 $100.00 · 钱包余额 $10.00')
  })
})

describe('Dashboard refresh controls', () => {
  it('does not render or run automatic refresh', async () => {
    vi.useFakeTimers()
    dashboardApiMocks.getStats.mockResolvedValue({ stats: [] })

    try {
      const root = mountDashboard()
      await settle()

      expect(root.textContent).not.toContain('自动刷新')
      expect(dashboardApiMocks.getStats).toHaveBeenCalledTimes(1)
      expect(dashboardApiMocks.getDailyStats).toHaveBeenCalledTimes(1)

      await vi.advanceTimersByTimeAsync(60_000)
      await settle()

      expect(dashboardApiMocks.getStats).toHaveBeenCalledTimes(1)
      expect(dashboardApiMocks.getDailyStats).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('Dashboard announcement placement and detail', () => {
  it('places notices before statistics and retains detail/read behavior', async () => {
    dashboardApiMocks.getStats.mockResolvedValue({ stats: [] })
    const notice = { id: 'notice', title: 'Gateway notice', content: 'Read this announcement', type: 'important', priority: 10, is_pinned: true, is_active: true, requires_ack: false, is_read: false, author: { id: 'admin', username: 'Admin' }, created_at: '2026-10-16T00:00:00Z' }
    announcementMocks.getAnnouncements.mockResolvedValue({ items: [notice] })
    const root = mountDashboard(); await settle()
    expect(root.firstElementChild?.firstElementChild?.id).toBe('announcements-section')
    root.querySelector<HTMLButtonElement>('.announcement-featured')!.click(); await settle()
    expect(announcementMocks.markAsRead).toHaveBeenCalledWith('notice')
    expect(root.textContent).toContain('Read this announcement')
    expect(root.querySelector('.announcement-featured__footer')?.textContent).toContain('已读')
  })

  it('shows source failure and retries without changing announcement data', async () => {
    dashboardApiMocks.getStats.mockResolvedValue({ stats: [] })
    announcementMocks.getAnnouncements.mockRejectedValueOnce(new Error('offline'))
    const root = mountDashboard(); await settle()
    expect(root.textContent).toContain('公告暂时无法加载')
    root.querySelector<HTMLButtonElement>('#announcements-section button')!.click(); await settle()
    expect(announcementMocks.getAnnouncements).toHaveBeenCalledTimes(2)
    expect(root.textContent).toContain('暂无公告')
    expect(announcementMocks.markAsRead).not.toHaveBeenCalled()
  })
})
