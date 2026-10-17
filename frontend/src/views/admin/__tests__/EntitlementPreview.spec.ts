import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, reactive, type App } from 'vue'
import EntitlementPreview from '../components/EntitlementPreview.vue'
const mocks = vi.hoisted(() => ({ preview: vi.fn(), keys: vi.fn() }))
vi.mock('@/api/access-preview', () => ({ previewAccess: mocks.preview }))
vi.mock('@/api/users', () => ({ usersApi: { getUserApiKeys: mocks.keys } }))
vi.mock('@/components/ui', () => ({ Button: defineComponent({ setup(_, { slots }) { return () => h('button', {}, slots.default?.()) } }) }))
let app: App | undefined, root: HTMLDivElement
afterEach(() => { app?.unmount(); root?.remove(); vi.clearAllMocks() })
async function flush() { for (let i = 0; i < 10; i++) { await Promise.resolve(); await nextTick() } }
const data = { enabled: true, user: { id: 'u', username: 'member', is_active: true }, model_access: { source: 'personal', allowed_models: ['public-model'] }, key: null, current: { source: 'group', quota: { windows: [], timezone: 'Asia/Shanghai', unlimited: false } }, proposed: { source: 'personal', quota: { windows: [], timezone: 'Asia/Shanghai', unlimited: true } }, changed: true }
describe('effective entitlement preview', () => {
  it('previews an unsaved draft without writing and clears results on edits', async () => {
    mocks.keys.mockResolvedValue([]); mocks.preview.mockResolvedValue(data)
    const draft = reactive({ timezone: 'Asia/Shanghai', groups: {}, users: {} })
    root = document.createElement('div'); document.body.append(root)
    app = createApp(EntitlementPreview, { users: [{ id: 'u', name: 'member' }], selectedUser: 'u', getDraft: () => draft }); app.mount(root); await flush()
    root.querySelector('button')!.click(); await flush()
    expect(mocks.preview).toHaveBeenCalledWith('u', draft, undefined, expect.any(AbortSignal))
    expect(root.textContent).toContain('草稿将改变此用户权益')
    expect(root.textContent).toContain('public-model')
    draft.timezone = 'UTC'; await flush()
    expect(root.textContent).not.toContain('草稿将改变此用户权益')
  })
  it('does not render stale responses after changing subjects or crash for incomplete drafts', async () => {
    mocks.keys.mockResolvedValue([])
    let resolve!: (value: unknown) => void
    mocks.preview.mockImplementation(() => new Promise(r => { resolve = r }))
    root = document.createElement('div'); document.body.append(root)
    const valid = reactive({ value: false })
    app = createApp(EntitlementPreview, { users: [{ id: 'u', name: 'member' }], selectedUser: 'u', getDraft: () => { if (!valid.value) throw Error('请选择家族'); return { timezone: 'UTC', groups: {}, users: {} } } }); app.mount(root); await flush()
    root.querySelector('button')!.click(); await flush()
    expect(root.textContent).toContain('请选择家族')
    valid.value = true; await flush(); root.querySelector('button')!.click(); await flush()
    const select = root.querySelector<HTMLSelectElement>('[aria-label="预览用户"]')!; select.value = ''; select.dispatchEvent(new Event('change')); await flush()
    resolve(data); await flush()
    expect(root.textContent).not.toContain('草稿将改变此用户权益')
  })
})
