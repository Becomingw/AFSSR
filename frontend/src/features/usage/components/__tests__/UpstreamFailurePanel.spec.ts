import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, reactive, type App } from 'vue'
import UpstreamFailurePanel from '../UpstreamFailurePanel.vue'
import type { RequestTrace } from '@/api/requestTrace'

const mocks = vi.hoisted(() => ({ trace: vi.fn(), detail: vi.fn(), copy: vi.fn() }))
vi.mock('@/api/requestTrace', () => ({ requestTraceApi: { getRequestTrace: mocks.trace } }))
vi.mock('@/api/dashboard', () => ({ dashboardApi: { getRequestDetail: mocks.detail } }))
vi.mock('@/composables/useClipboard', () => ({ useClipboard: () => ({ copyToClipboard: mocks.copy }) }))
vi.mock('@/components/ui/button.vue', () => ({
  default: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }),
}))

let app: App
let root: HTMLDivElement
const props = reactive({ requestId: 'req-1' as string | null, usageId: 'usage-1' as string | null })

const failedTrace = (diagnosticState: string | null = 'reference'): RequestTrace => ({
  request_id: 'req-1',
  total_candidates: 2,
  final_status: 'failed',
  total_latency_ms: 4,
  diagnostic_request: { usage_id: 'usage-1', body_state: diagnosticState },
  candidates: [
    {
      id: 'a1', request_id: 'req-1', candidate_index: 0, retry_index: 0, status: 'failed', is_cached: false,
      status_code: 429, created_at: 'x', provider_name: 'Acme',
      extra_data: {
        client_api_format: 'openai:chat',
        provider_api_format: 'openai:chat',
        upstream_response: { status_code: 429, headers: { 'retry-after': '7' }, body: { error: { message: 'first' } } },
      },
    },
    {
      id: 'a2', request_id: 'req-1', candidate_index: 0, retry_index: 1, status: 'failed', is_cached: false,
      status_code: 400, created_at: 'x', provider_name: 'Acme', error_message: 'unsupported parameter',
      extra_data: {
        client_api_format: 'openai:responses',
        provider_api_format: 'openai:chat',
        upstream_url: 'http://up.example/',
        upstream_response: { status_code: 400, headers: { 'x-request-id': 'up-1' }, body: { error: { message: 'unsupported parameter' } } },
        error_flow: { decision: 'retry_next_candidate', retryable: true },
      },
    },
  ],
})

async function flush() { for (let i = 0; i < 10; i++) { await Promise.resolve(); await nextTick() } }

beforeEach(() => {
  vi.clearAllMocks()
  props.requestId = 'req-1'
  props.usageId = 'usage-1'
  mocks.trace.mockResolvedValue(failedTrace())
  mocks.detail.mockResolvedValue({ provider_request_body: { model: 'm', reasoning_effort: 'high' }, has_provider_request_body: true })
  root = document.createElement('div')
  document.body.append(root)
  app = createApp({ render: () => h(UpstreamFailurePanel, { ...props }) })
  app.mount(root)
})
afterEach(() => { app.unmount(); root.remove() })

describe('UpstreamFailurePanel', () => {
  it('shows the request sent upstream next to the error the upstream returned', async () => {
    await flush()
    expect(mocks.trace).toHaveBeenCalledWith('req-1', { attemptedOnly: true })
    expect(mocks.detail).toHaveBeenCalledWith('usage-1', expect.objectContaining({ includeBodies: true }))
    // The last failed attempt decided the outcome, so it is selected first.
    expect(root.querySelector('[data-testid="upstream-status"]')!.textContent).toBe('400')
    expect(root.querySelector('[data-testid="upstream-request-body"]')!.textContent).toContain('reasoning_effort')
    expect(root.querySelector('[data-testid="upstream-error-body"]')!.textContent).toContain('unsupported parameter')
    expect(root.textContent).toContain('openai:responses → openai:chat')
    expect(root.textContent).toContain('换下一个候选重试')
    expect(root.textContent).toContain('x-request-id')
  })

  it('switches between failed attempts', async () => {
    await flush()
    const tabs = [...root.querySelectorAll<HTMLButtonElement>('[role="tab"]')]
    expect(tabs).toHaveLength(2)
    tabs[0].click()
    await flush()
    expect(root.querySelector('[data-testid="upstream-status"]')!.textContent).toBe('429')
    expect(root.textContent).toContain('retry-after')
  })

  it('explains how to capture the request when bodies were not recorded', async () => {
    mocks.detail.mockResolvedValue({ provider_request_body: null, has_provider_request_body: false })
    mocks.trace.mockResolvedValue(failedTrace('disabled'))
    props.requestId = 'req-2'
    await flush()
    expect(root.querySelector('[data-testid="upstream-request-disabled"]')!.textContent).toContain('FAILURES')
    expect(root.querySelector('[data-testid="upstream-request-body"]')).toBeNull()
    // The upstream error is still shown without any body capture.
    expect(root.querySelector('[data-testid="upstream-status"]')!.textContent).toBe('400')
  })

  it('falls back to the client request for same-format passthrough, which stores one copy', async () => {
    mocks.detail.mockResolvedValue({
      provider_request_body: null,
      has_provider_request_body: false,
      has_format_conversion: false,
      request_body: { model: 'm', stream: true },
    })
    props.requestId = 'req-5'
    await flush()
    expect(root.querySelector('[data-testid="upstream-request-body"]')!.textContent).toContain('"stream": true')
    expect(root.querySelector('[data-testid="upstream-request-passthrough"]')).not.toBeNull()
    expect(root.querySelector('[data-testid="upstream-request-disabled"]')).toBeNull()
  })

  it('says so when no attempt reached an upstream, and does not leak API errors', async () => {
    mocks.trace.mockResolvedValue({ ...failedTrace(), candidates: [] })
    props.requestId = 'req-3'
    await flush()
    expect(root.textContent).toContain('没有到达上游的失败记录')
    mocks.trace.mockRejectedValue(new Error('CANARY Bearer secret'))
    props.requestId = 'req-4'
    await flush()
    expect(root.textContent).toContain('无法读取上游失败详情')
    expect(root.textContent).not.toContain('CANARY')
  })

  it('copies the raw error body', async () => {
    await flush()
    const copy = [...root.querySelectorAll('button')].filter(b => b.textContent?.includes('复制'))[1]
    copy.click()
    expect(mocks.copy).toHaveBeenCalledWith(expect.stringContaining('unsupported parameter'))
  })
})
