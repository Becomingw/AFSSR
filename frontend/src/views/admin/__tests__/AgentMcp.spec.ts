import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import AgentMcp from '../AgentMcp.vue'
import { canAccessAdminPage } from '@/config/adminRoles'

const mocks = vi.hoisted(() => ({ list: vi.fn(), create: vi.fn(), revoke: vi.fn(), update: vi.fn() }))
vi.mock('@/api/mcp', () => ({ listMcpKeys: mocks.list, createMcpKey: mocks.create, revokeMcpKey: mocks.revoke, updateMcpKey: mocks.update }))
vi.mock('@/components/ui', () => ({
  Button: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }),
  Input: defineComponent({ props: { modelValue: { type: [String, Number], default: '' } }, emits: ['update:modelValue'], setup(props, { emit, attrs }) { return () => h('input', { ...attrs, value: props.modelValue, onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value) }) } }),
}))
vi.mock('@/components/ui/dialog/Dialog.vue', () => ({ default: defineComponent({ props: { open: Boolean, title: { type: String, default: '' } }, setup(props, { slots }) { return () => props.open ? h('section', { 'data-dialog': props.title }, slots.default?.()) : null } }) }))

let app: App, root: HTMLDivElement
async function flush() { for (let i = 0; i < 8; i++) { await Promise.resolve(); await nextTick() } }
function click(label: string) { const button = [...root.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.trim() === label); expect(button).toBeTruthy(); button!.click() }
function input(id: string, value: string) { const field = root.querySelector<HTMLInputElement>(`#${id}`)!; field.value = value; field.dispatchEvent(new Event('input', { bubbles: true })) }
const key = { id: 'fixture-id', name: 'MCP/Research', token_prefix: 'fixture', is_active: true, permissions: ['admin:users:read'] }
beforeEach(async () => {
  vi.resetAllMocks()
  mocks.list.mockResolvedValue({ items: [key], user_id: 'user-fixture', role: 'user_admin', read_permissions: ['admin:users:read'], write_permissions: ['admin:users:read', 'admin:users:write'], endpoint: '/mcp', enabled: true, max_keys: 20 })
  mocks.create.mockResolvedValue({ token: 'test-only-one-time-secret', data: key })
  mocks.revoke.mockResolvedValue(undefined)
  mocks.update.mockResolvedValue({ data: key })
  root = document.createElement('div'); document.body.append(root)
  app = createApp(AgentMcp); app.mount(root); await flush()
})
afterEach(() => { app.unmount(); root.remove() })
describe('Agent MCP console', () => {
  it('is visible to four administrator roles but never ordinary users', () => {
    for (const role of ['admin', 'audit_admin', 'user_admin', 'service_admin']) expect(canAccessAdminPage(role, '/admin/mcp')).toBe(true)
    expect(canAccessAdminPage('user', '/admin/mcp')).toBe(false)
    expect(root.textContent).toContain('用户及权益管理员')
    expect(root.textContent).toContain('Research')
    expect(root.textContent).not.toContain('test-only-one-time-secret')
  })
  it('defaults to readonly and shows a new secret only until the dialog closes', async () => {
    const storage = vi.spyOn(Storage.prototype, 'setItem')
    click('创建 API Key'); await flush(); input('mcp-key-name', 'Agent'); await flush()
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await flush()
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ name: 'Agent', read_only: true, expires_in_days: 30 }))
    expect(root.querySelector('[data-testid="mcp-new-secret"]')?.textContent).toBe('test-only-one-time-secret')
    expect(root.querySelector('pre')?.textContent).not.toContain('test-only-one-time-secret')
    expect(storage).not.toHaveBeenCalled()
    click('关闭'); await flush()
    expect(root.textContent).not.toContain('test-only-one-time-secret')
    storage.mockRestore()
  })
  it('requires an explicit revoke confirmation', async () => {
    root.querySelector<HTMLButtonElement>('button[title="撤销 MCP/Research"]')!.click(); await flush()
    expect(mocks.revoke).not.toHaveBeenCalled()
    click('撤销'); await flush()
    expect(mocks.revoke).toHaveBeenCalledWith('fixture-id')
  })
  it('clears stale configuration and disables creation after a failed reload', async () => {
    mocks.list.mockRejectedValue(new Error('offline'))
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('未连接')
    expect([...root.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent?.includes('创建 API Key'))?.disabled).toBe(true)
  })
  it('does not offer writes to an audit administrator', async () => {
    mocks.list.mockResolvedValue({ items: [], user_id: 'audit', role: 'audit_admin', read_permissions: ['admin:users:read'], write_permissions: ['admin:users:read'], endpoint: '/mcp', enabled: true })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    click('创建 API Key'); await flush()
    expect(root.querySelector<HTMLInputElement>('input[value="write"]')?.disabled).toBe(true)
  })
  it('issues a feedback-only worker key without granting unrelated administration', async () => {
    mocks.list.mockResolvedValue({ items: [], user_id: 'audit', role: 'audit_admin', read_permissions: ['admin:feedback:read', 'admin:users:read'], write_permissions: ['admin:feedback:read', 'admin:feedback:write', 'admin:users:read'], endpoint: '/mcp', enabled: true })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush(); click('创建 API Key'); await flush()
    const select = root.querySelector<HTMLSelectElement>('#mcp-key-purpose')!; select.value = 'feedback'; select.dispatchEvent(new Event('change', { bubbles: true }))
    const write = root.querySelector<HTMLInputElement>('input[value="write"]')!; write.checked = true; write.dispatchEvent(new Event('change', { bubbles: true }))
    input('mcp-key-name', 'Feedback worker'); await flush()
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await flush()
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ read_only: false, permissions: ['admin:feedback:read', 'admin:feedback:write'] }))
  })
})

describe('MCP key lifecycle', () => {
  function submit() { root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })) }
  function check(id: string, checked: boolean) { const field = root.querySelector<HTMLInputElement>(`#${id}`)!; field.checked = checked; field.dispatchEvent(new Event('change', { bubbles: true })) }
  async function edit() { root.querySelector<HTMLButtonElement>('button[title="修改有效期 MCP/Research"]')!.click(); await flush() }
  it('creates a never-expiring key only with explicit selection', async () => {
    click('创建 API Key'); await flush(); input('mcp-key-name', 'Permanent'); check('mcp-key-never', true); await flush(); submit(); await flush()
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ expires_in_days: null }))
  })
  it('renews an existing key and can switch to never expiring', async () => {
    await edit(); check('mcp-edit-never', false); await flush(); input('mcp-edit-days', '45'); await flush(); submit(); await flush()
    expect(mocks.update).toHaveBeenCalledWith('fixture-id', { expires_in_days: 45 })
    await edit(); check('mcp-edit-never', true); await flush(); submit(); await flush()
    expect(mocks.update).toHaveBeenLastCalledWith('fixture-id', { expires_in_days: null })
  })
  it('rejects invalid expiry without sending a mutation', async () => {
    await edit(); check('mcp-edit-never', false); await flush(); input('mcp-edit-days', '0'); await flush(); submit(); await flush()
    expect(mocks.update).not.toHaveBeenCalled()
    expect(root.querySelector('[data-dialog="修改有效期"] [role="alert"]')?.textContent).toContain('1–90')
  })
  it('shows backend update errors inside the open dialog', async () => {
    mocks.update.mockRejectedValue({ response: { data: { message: 'Key not found' } } })
    await edit(); submit(); await flush()
    expect(root.querySelector('[data-dialog="修改有效期"] [role="alert"]')?.textContent).toContain('Key not found')
  })
  it('shows backend revoke errors inside the confirmation and clears them when reopened', async () => {
    mocks.revoke.mockRejectedValue({ response: { data: { message: 'Session expired' } } })
    root.querySelector<HTMLButtonElement>('button[title="撤销 MCP/Research"]')!.click(); await flush(); click('撤销'); await flush()
    expect(root.querySelector('[data-dialog="撤销 API Key"] [role="alert"]')?.textContent).toContain('Session expired')
    click('取消'); await flush(); root.querySelector<HTMLButtonElement>('button[title="撤销 MCP/Research"]')!.click(); await flush()
    expect(root.querySelector('[data-dialog="撤销 API Key"] [role="alert"]')).toBeNull()
  })
  it('reports successful revocation separately from a failed refresh', async () => {
    mocks.list.mockRejectedValue(new Error('offline'))
    root.querySelector<HTMLButtonElement>('button[title="撤销 MCP/Research"]')!.click(); await flush(); click('撤销'); await flush()
    expect(root.querySelector('[data-dialog="撤销 API Key"]')).toBeNull()
    expect(root.textContent).toContain('密钥已撤销，但列表刷新失败')
    expect(mocks.revoke).toHaveBeenCalledTimes(1)
  })
  it('reports a successful expiry update separately from refresh failure', async () => {
    mocks.list.mockRejectedValue(new Error('offline'))
    await edit(); submit(); await flush()
    expect(root.querySelector('[data-dialog="修改有效期"]')).toBeNull()
    expect(root.textContent).toContain('有效期已更新，但列表刷新失败')
  })
  it('does not update a key when editing is cancelled', async () => {
    await edit(); click('取消'); await flush()
    expect(mocks.update).not.toHaveBeenCalled()
    expect(root.querySelector('[data-dialog="修改有效期"]')).toBeNull()
  })
  it('keeps a newly created secret when the list refresh fails', async () => {
    mocks.list.mockRejectedValue(new Error('offline'))
    click('创建 API Key'); await flush(); input('mcp-key-name', 'Agent'); await flush(); submit(); await flush()
    expect(root.querySelector('[data-testid="mcp-new-secret"]')?.textContent).toBe('test-only-one-time-secret')
    expect(root.textContent).toContain('密钥已创建，但列表刷新失败')
  })
  it('prevents repeated renewal and cancellation while a mutation is pending', async () => {
    let resolve!: (value: unknown) => void
    mocks.update.mockImplementation(() => new Promise(r => { resolve = r }))
    await edit(); submit(); submit(); await flush()
    expect(mocks.update).toHaveBeenCalledTimes(1)
    expect([...root.querySelectorAll<HTMLButtonElement>('[data-dialog="修改有效期"] button')].every(b => b.disabled)).toBe(true)
    resolve({ data: key }); await flush()
    expect(root.querySelector('[data-dialog="修改有效期"]')).toBeNull()
  })
  it('allows expired keys to be revoked', async () => {
    mocks.list.mockResolvedValue({ items: [{ ...key, expires_at_unix_secs: 1 }], endpoint: '/mcp', enabled: true, role: 'admin', read_permissions: [], write_permissions: [] })
    root.querySelector<HTMLButtonElement>('button[title="刷新"]')!.click(); await flush()
    expect(root.textContent).toContain('已失效')
    root.querySelector<HTMLButtonElement>('button[title="撤销 MCP/Research"]')!.click(); await flush(); click('撤销'); await flush()
    expect(mocks.revoke).toHaveBeenCalledWith('fixture-id')
  })
})
