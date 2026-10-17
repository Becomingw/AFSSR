import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import Compatibility from '../Compatibility.vue'
import { canAccessAdminPage } from '@/config/adminRoles'

const api = vi.hoisted(() => ({ list: vi.fn(), saveProfile: vi.fn(), run: vi.fn() }))
const channels = vi.hoisted(() => ({ providers: vi.fn(), models: vi.fn(), endpoints: vi.fn() }))
vi.mock('@/api/endpoints/providers', () => ({ getProvidersSummary: channels.providers }))
vi.mock('@/api/endpoints/models', () => ({ getProviderModels: channels.models }))
vi.mock('@/api/endpoints/endpoints', () => ({ getProviderEndpoints: channels.endpoints }))
vi.mock('@/api/compatibility', async importOriginal => ({ ...await importOriginal<object>(), compatibilityApi: api }))
vi.mock('@/components/ui', () => ({
  Button: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }),
  Input: defineComponent({ props: { modelValue: { type: [String, Number], default: '' } }, emits: ['update:modelValue'], setup(props, { emit, attrs }) { return () => h('input', { ...attrs, value: props.modelValue, onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value) }) } }),
}))
vi.mock('@/components/layout', () => ({
  PageContainer: defineComponent({ setup(_, { slots }) { return () => h('main', slots.default?.()) } }),
  PageHeader: defineComponent({ props: { title: String }, setup(props, { slots }) { return () => h('header', [props.title, slots.actions?.()]) } }),
}))
const profile = { id: 'fixture-id', config_fingerprint: 'abcdef0123456789', config: { client: 'codex', client_version: 'synthetic-1', model: 'fixture-model', mapped_model: 'fixture-upstream', client_protocol: 'openai:responses', upstream_format: 'claude:messages' } }
const run = { id: 'run-id', profile_id: profile.id, config: profile.config, config_fingerprint: 'old-fingerprint', suite: 'offline-protocol-v1', evidence: 'offline_synthetic', created_at: 1, cases: [
  { case: 'basic_request', status: 'failed', reason: 'Protocol conversion rejected the synthetic fixture' },
  { case: 'compression', status: 'not_tested', reason: 'Not executed' },
  { case: 'stream', status: 'future_status', reason: 'Future result' },
] }
function snapshot(can_run = true) { return { schema_version: 1, revision: 3, can_run, profiles: [profile], runs: [{ run, stale: true }] } }
let app: App, root: HTMLDivElement
async function flush() { for (let i = 0; i < 8; i++) { await Promise.resolve(); await nextTick() } }
function tab(label: string) { return [...root.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find(b => b.textContent?.includes(label))! }
function button(label: string) { return [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent?.trim() === label)! }
beforeEach(async () => {
  vi.clearAllMocks(); api.list.mockResolvedValue(snapshot()); api.run.mockResolvedValue(snapshot()); api.saveProfile.mockResolvedValue(snapshot())
  channels.providers.mockResolvedValue({ total: 1, items: [{ id: 'provider-1', name: 'Test channel', is_active: true }] })
  channels.models.mockResolvedValue([{ id: 'model-1', global_model_name: 'public-model', provider_model_name: 'upstream-model' }])
  channels.endpoints.mockResolvedValue([{ id: 'endpoint-1', api_format: 'claude:messages', is_active: true }])
  root = document.createElement('div'); document.body.append(root); app = createApp(Compatibility); app.mount(root); await flush()
})
afterEach(() => { app.unmount(); root.remove() })
describe('compatibility workbench', () => {
  it('selects a channel model and endpoint without typing protocol or model identifiers', async () => {
    button('新建配置').click(); await flush()
    const provider = root.querySelector<HTMLSelectElement>('[aria-label="测试渠道"]')!
    provider.value = 'provider-1'; provider.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    const model = root.querySelector<HTMLSelectElement>('[aria-label="测试渠道模型"]')!
    model.value = 'model-1'; model.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    expect(root.querySelector<HTMLInputElement>('[aria-label="模型标识"]')!.value).toBe('public-model')
    expect(root.querySelector<HTMLInputElement>('[aria-label="映射模型标识"]')!.value).toBe('upstream-model')
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await flush()
    expect(api.saveProfile).toHaveBeenCalledWith(3, expect.objectContaining({ config: expect.objectContaining({ model: 'public-model', mapped_model: 'upstream-model', upstream_format: 'claude:messages', channel: { provider_id: 'provider-1', provider_model_id: 'model-1', endpoint_id: 'endpoint-1' } }) }))
    expect(root.textContent).toContain('真实模型：未测试')
  })
  it('allows all four administrator roles to read, never users', () => {
    for (const role of ['admin', 'audit_admin', 'user_admin', 'service_admin']) expect(canAccessAdminPage(role, '/admin/compatibility')).toBe(true)
    expect(canAccessAdminPage('user', '/admin/compatibility')).toBe(false)
    expect(root.textContent).toContain('已过期')
    expect(root.textContent).toContain('真实模型：未测试')
    expect(root.textContent).toContain('真实客户端：未测试')
  })
  it('never renders unknown status as a pass and displays failure reasons', async () => {
    tab('合成记录').click(); await flush()
    button('逐项结果').click(); await flush()
    expect(root.querySelector('[aria-label="逐项结果"]')?.textContent).toContain('已过期')
    expect(root.textContent).toContain('未知（非通过）')
    expect(root.textContent).toContain('Protocol conversion rejected')
    expect(root.textContent).toContain('未测试')
  })
  it('runs with the current revision and fingerprint only when permitted', async () => {
    button('合成验收').click(); await flush()
    expect(api.run).toHaveBeenCalledWith(3, profile)
    api.list.mockResolvedValue(snapshot(false)); root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(button('合成验收').disabled).toBe(true)
    expect(button('新建配置').disabled).toBe(true)
  })
  it('filters model and results and clears stale UI on a failed refresh', async () => {
    const input = root.querySelector<HTMLInputElement>('[aria-label="筛选模型"]')!
    input.value = 'nonmatching'; input.dispatchEvent(new Event('input', { bubbles: true })); await flush()
    expect(root.textContent).toContain('无匹配配置')
    api.list.mockRejectedValue(new Error('Offline storage unavailable'))
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('Offline storage unavailable')
    expect(button('新建配置').disabled).toBe(true)
    expect(root.textContent).not.toContain('old-fingerprint')
  })
  it('does not silently retry revision conflicts', async () => {
    api.run.mockRejectedValue({ response: { status: 409 } })
    button('合成验收').click(); await flush()
    expect(api.run).toHaveBeenCalledTimes(1)
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('配置已变动')
    expect(button('新建配置').disabled).toBe(true)
  })
  it('summarises configs, opens the latest result from a card and lists every scenario', async () => {
    expect(root.querySelector('[aria-label="概览"]')?.textContent).toContain('待运行 / 已过期')
    expect(root.querySelector('[aria-label="配置列表"] article')?.textContent).toContain('fixture-model')
    button('最近结果').click(); await flush()
    const detail = root.querySelector('[aria-label="逐项结果"]')!
    expect(detail.textContent).toContain('基础请求')
    expect(detail.textContent).toContain('Protocol conversion rejected')
    root.querySelector<HTMLButtonElement>('button[title="关闭"]')!.click(); await flush()
    expect(root.querySelector('[aria-label="逐项结果"]')).toBeNull()
  })
  it('shows the coverage matrix with one row per config and never marks unknown results as passed', async () => {
    tab('覆盖矩阵').click(); await flush()
    const matrix = root.querySelector('[aria-label="覆盖矩阵"]')!
    expect(matrix.querySelectorAll('tbody tr')).toHaveLength(1)
    expect(matrix.querySelectorAll('thead th')).toHaveLength(10)
    expect(matrix.textContent).toContain('fixture-model')
    expect(matrix.querySelectorAll('button[title*="合成通过"]')).toHaveLength(0)
  })
  it('runs every visible config in sequence against the newest revision', async () => {
    const second = { ...profile, id: 'second-id', config_fingerprint: 'second-fingerprint' }
    api.list.mockResolvedValue({ ...snapshot(), profiles: [profile, second] }); root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    api.run.mockResolvedValueOnce({ ...snapshot(), revision: 4, profiles: [profile, second] }).mockResolvedValueOnce({ ...snapshot(), revision: 5, profiles: [profile, second] })
    button('运行全部').click(); await flush()
    expect(api.run).toHaveBeenCalledTimes(2)
    expect(api.run.mock.calls.map(call => [call[0], call[1].id])).toEqual([[3, 'fixture-id'], [4, 'second-id']])
  })
  it('duplicates a config as a new one instead of overwriting it', async () => {
    root.querySelector<HTMLButtonElement>('button[title="复制为新配置"]')!.click(); await flush()
    expect(root.querySelector('[aria-label="配置表单"]')?.textContent).toContain('新建配置')
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await flush()
    const saved = api.saveProfile.mock.calls[0]![1]
    expect(saved.id).not.toBe(profile.id)
  })
})
