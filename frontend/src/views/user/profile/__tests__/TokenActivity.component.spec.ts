import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App } from 'vue'
import TokenActivity from '../TokenActivity.vue'

const get = vi.hoisted(() => vi.fn())
vi.mock('@/api/me', () => ({ meApi: { getActivityHeatmap: get } }))
let app: App, root: HTMLDivElement
const fixture = { start_date: '2026-10-10', end_date: '2026-10-16', total_days: 7, max_requests: 99, days: [{ date: '2026-10-11', requests: 99, total_tokens: 10, total_cost: 0 }, { date: '2026-10-12', requests: 1, total_tokens: 90, total_cost: 0 }] }
async function flush() { for (let i = 0; i < 12; i++) { await Promise.resolve(); await nextTick() } }
function click(label: string) { const button = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === label); expect(button).toBeTruthy(); button?.click() }
beforeEach(async () => { vi.clearAllMocks(); get.mockResolvedValue(structuredClone(fixture)); root = document.createElement('div'); document.body.append(root); app = createApp(TokenActivity); app.mount(root); await flush() })
afterEach(() => { app.unmount(); root.remove() })
describe('Token activity profile section', () => {
  it('shows neutral empty days, colors tokens rather than requests, and keeps the year independent of overview filters', () => {
    expect(get).toHaveBeenCalledWith(false)
    const cells = root.querySelectorAll<HTMLButtonElement>('button.activity-cell')
    expect(cells).toHaveLength(7)
    expect(cells[0]?.getAttribute('data-level')).toBe('0')
    expect(cells[1]?.getAttribute('data-level')).toBe('2')
    expect(cells[2]?.getAttribute('data-level')).toBe('4')
    expect(root.textContent).toContain('100 Token · 有用量 2 天')
    expect(root.textContent).toContain('UTC')
    expect(root.querySelectorAll('button.activity-cell[tabindex="0"]')).toHaveLength(1)
  })
  it('switches weekly and period cumulative views without additional requests', async () => {
    click('每周'); await flush()
    const weeks = root.querySelectorAll<HTMLButtonElement>('.weekly-bar')
    expect(weeks).toHaveLength(2)
    expect(weeks[0]?.getAttribute('aria-label')).toContain('2026-10-10 至 2026-10-11，10 Token')
    expect(weeks[1]?.getAttribute('aria-label')).toContain('90 Token')
    click('累计总量'); await flush()
    expect(root.querySelector('[data-point="6"]')?.getAttribute('aria-label')).toContain('累计 100 Token')
    expect(root.textContent).toContain('区间内累计')
    expect(get).toHaveBeenCalledTimes(1)
  })
  it('supports keyboard focus, exact-value tooltips and Escape dismissal', async () => {
    const last = root.querySelector<HTMLButtonElement>('[data-point="6"]')!
    last.focus(); await flush()
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('2026-10-16')
    expect(document.querySelector('[role="tooltip"]')?.textContent).toContain('0 Token')
    last.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true })); await flush()
    const first = root.querySelector<HTMLButtonElement>('[data-point="0"]')!
    expect(document.activeElement).toBe(first)
    first.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await flush()
    expect(document.querySelector('[role="tooltip"]')).toBeNull()
    first.focus(); window.dispatchEvent(new Event('scroll')); await flush()
    expect(document.querySelector('[role="tooltip"]')).toBeNull()
  })
  it('refreshes and retries errors without disguising missing data as zero activity', async () => {
    get.mockRejectedValueOnce(new Error('offline'))
    root.querySelector<HTMLButtonElement>('[aria-label="刷新 Token 活动"]')!.click(); await flush()
    expect(root.querySelector('[role="alert"]')).toBeTruthy()
    expect(root.querySelector('.activity-chart')).toBeNull()
    click('重试'); await flush()
    expect(get).toHaveBeenLastCalledWith(true)
    expect(root.querySelector('.activity-chart')).toBeTruthy()
  })
  it('repositions keyboard tooltips after native focus scrolling without undoing Escape', async () => {
    const cell = root.querySelector<HTMLButtonElement>('[data-point="6"]')!
    const rect = { x: 100, y: 100, width: 12, height: 12, top: 100, left: 100, right: 112, bottom: 112, toJSON: () => ({}) }
    const bounds = vi.spyOn(cell, 'getBoundingClientRect').mockReturnValue(rect)
    cell.focus(); await flush()
    bounds.mockReturnValue({ ...rect, y: 150, top: 150, bottom: 162 })
    window.dispatchEvent(new Event('scroll')); await flush()
    expect(document.querySelector<HTMLElement>('[role="tooltip"]')?.style.top).toBe('142px')
    cell.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await flush()
    window.dispatchEvent(new Event('scroll')); await flush()
    expect(document.querySelector('[role="tooltip"]')).toBeNull()
  })
  it('ignores completion after unmount and removes the tooltip', async () => {
    root.querySelector<HTMLButtonElement>('[data-point="6"]')!.focus(); await flush()
    expect(document.querySelector('[role="tooltip"]')).toBeTruthy()
    let resolve!: (value: unknown) => void
    get.mockImplementationOnce(() => new Promise(done => { resolve = done }))
    root.querySelector<HTMLButtonElement>('[aria-label="刷新 Token 活动"]')!.click(); await flush()
    app.unmount(); resolve(fixture); await flush()
    expect(document.querySelector('[role="tooltip"]')).toBeNull()
    app = createApp({ render: () => null }); app.mount(root)
  })
})
