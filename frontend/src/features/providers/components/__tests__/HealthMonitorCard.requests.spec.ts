import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import { createI18n } from '@/i18n'
import HealthMonitorCard from '../HealthMonitorCard.vue'

const mocks = vi.hoisted(() => ({ load: vi.fn(), error: vi.fn() }))
vi.mock('@/api/endpoints/health', () => ({ getEndpointStatusMonitor: mocks.load, getPublicEndpointStatusMonitor: mocks.load }))
vi.mock('@/composables/useToast', () => ({ useToast: () => ({ error: mocks.error }) }))
vi.mock('../HealthMonitorHeader.vue', () => ({ default: defineComponent({
  emits: ['update:lookbackHours'],
  setup(_, { emit }) { return () => h('button', { onClick: () => emit('update:lookbackHours', '24') }, 'change range') },
}) }))
vi.mock('../EndpointHealthTimeline.vue', () => ({ default: defineComponent({ setup() { return () => h('div') } }) }))

let app: App | null, root: HTMLDivElement
const requests: Array<{ signal: AbortSignal; resolve: (value: unknown) => void; reject: (reason: unknown) => void }> = []
async function flush() { for (let n = 0; n < 8; n++) { await Promise.resolve(); await nextTick() } }
function data(rate: number) { return { formats: [{ api_format: 'openai:responses', total_attempts: 100,
  success_count: rate * 100, failed_count: (1 - rate) * 100, success_rate: rate, provider_count: 1, key_count: 1, events: [] }] } }
beforeEach(() => {
  requests.length = 0
  mocks.error.mockReset(); mocks.load.mockReset()
  mocks.load.mockImplementation((_params, signal) => new Promise((resolve, reject) => { requests.push({ signal, resolve, reject }) }))
  root = document.createElement('div'); document.body.appendChild(root)
  app = createApp(HealthMonitorCard, { isAdmin: true }); app.use(createI18n()); app.mount(root)
})
afterEach(() => { app?.unmount(); root.remove() })

describe('health monitoring request lifetime', () => {
  it('cancels the old range and refuses a late stale response', async () => {
    root.querySelector<HTMLButtonElement>('button')!.click(); await flush()
    expect(requests).toHaveLength(2)
    expect(requests[0].signal.aborted).toBe(true)
    requests[1].resolve(data(0.8)); await flush()
    expect(root.textContent).toContain('80.00%')
    requests[0].resolve(data(1)); await flush()
    expect(root.textContent).toContain('80.00%')
    expect(root.textContent).not.toContain('100.00%')
    expect(mocks.error).not.toHaveBeenCalled()
  })
  it('cancels on unmount without showing a spurious network failure', async () => {
    app!.unmount(); app = null
    expect(requests[0].signal.aborted).toBe(true)
    requests[0].reject(new Error('request canceled')); await flush()
    expect(mocks.error).not.toHaveBeenCalled()
  })
})
