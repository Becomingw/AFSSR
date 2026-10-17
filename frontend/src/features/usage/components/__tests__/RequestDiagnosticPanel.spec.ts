import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, reactive, type App } from 'vue'
import RequestDiagnosticPanel from '../RequestDiagnosticPanel.vue'
import type { RequestDiagnostic } from '@/api/diagnostics'
const mocks = vi.hoisted(() => ({ get: vi.fn(), feedback: vi.fn() }))
vi.mock('@/api/diagnostics', () => ({ getRequestDiagnostic: mocks.get }))
vi.mock('@/components/ui/button.vue', () => ({ default: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }) }))
let app: App, root: HTMLDivElement
const props = reactive({ requestId: 'req-1' as string | null, admin: false })
const result: RequestDiagnostic = { schema_version: 1, request_id: 'req-1', state: 'unknown', final_outcome: 'unknown', has_attempt_failures: false, summary: 'RequestID: req-1\nDiagnostic v1: unknown', stages: [{ id: 'stream', status: 'unknown', code: 'not_recorded', evidence: [] }] }
async function flush() { for (let i = 0; i < 8; i++) { await Promise.resolve(); await nextTick() } }
beforeEach(() => {
  vi.clearAllMocks(); props.requestId = 'req-1'; props.admin = false
  mocks.get.mockResolvedValue(result)
  root = document.createElement('div'); document.body.append(root)
  app = createApp({ render: () => h(RequestDiagnosticPanel, { ...props, onFeedback: mocks.feedback }) }); app.mount(root)
})
afterEach(() => { app.unmount(); root.remove() })
function read() { root.querySelector<HTMLButtonElement>('button[title="读取诊断"]')!.click() }
describe('safe request diagnosis', () => {
  it('is explicit and presents missing/running evidence as unknown, never successful', async () => {
    expect(mocks.get).not.toHaveBeenCalled(); read(); await flush()
    expect(root.textContent).toContain('最终结果未知'); expect(root.textContent).toContain('未知')
    const feedback = [...root.querySelectorAll('button')].find(b => b.textContent?.includes('带入主题'))!
    expect(feedback.disabled).toBe(true); expect(mocks.feedback).not.toHaveBeenCalled()
    props.requestId = 'req-2'; await flush(); expect(root.textContent).not.toContain('最终结果未知')
    mocks.get.mockResolvedValue({ ...result, request_id: 'req-2', state: 'in_progress' }); read(); await flush()
    expect(root.textContent).toContain('运行中，结果未知'); expect(root.textContent).not.toContain('已记录完成')
  })
  it('discards a late result for another request and does not leak API errors', async () => {
    let resolve!: (value: RequestDiagnostic) => void
    mocks.get.mockImplementationOnce(() => new Promise<RequestDiagnostic>(r => { resolve = r }))
    read(); await flush(); props.requestId = 'req-2'; await flush(); resolve(result); await flush()
    expect(root.textContent).not.toContain('req-1')
    mocks.get.mockRejectedValue(new Error('CANARY Bearer secret http://10.0.0.1'))
    read(); await flush(); expect(root.textContent).toContain('无法读取诊断'); expect(root.textContent).not.toContain('CANARY')
  })
  it('hands off only the request ID after an explicit feedback action', async () => {
    mocks.get.mockResolvedValue({ ...result, state: 'failed' }); read(); await flush()
    const button = [...root.querySelectorAll('button')].find(b => b.textContent?.includes('带入主题'))!; button.click()
    expect(mocks.feedback).toHaveBeenCalledWith('req-1')
    expect(mocks.get).toHaveBeenCalledWith(false, 'req-1', expect.any(AbortSignal))
  })
  it('shows completed fallback separately from retained attempt failures', async () => {
    mocks.get.mockResolvedValue({ ...result, state: 'known', final_outcome: 'completed', has_attempt_failures: true, stages: [{ id: 'upstream', status: 'failed', code: 'invalid_request_error', evidence: ['candidates.error_type'] }] })
    read(); await flush()
    expect(root.textContent).toContain('已记录完成'); expect(root.textContent).toContain('请求最终结果：已完成')
    expect(root.textContent).toContain('失败'); expect(root.textContent).toContain('invalid_request_error')
    expect(root.textContent).not.toContain('运行中')
  })
})
