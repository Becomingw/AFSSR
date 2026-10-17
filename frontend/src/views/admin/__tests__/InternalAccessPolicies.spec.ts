import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, type App } from 'vue'
import InternalAccessPolicies from '../InternalAccessPolicies.vue'

const mocks = vi.hoisted(() => ({ load: vi.fn(), save: vi.fn(), groups: vi.fn(), users: vi.fn(), models: vi.fn(), keys: vi.fn(), preview: vi.fn(), canManage: true }))
vi.mock('@/stores/auth', () => ({ useAuthStore: () => ({ get canManageUsers() { return mocks.canManage } }) }))
vi.mock('@/api/internal-access', async importOriginal => ({
  ...await importOriginal<typeof import('@/api/internal-access')>(),
  loadInternalAccess: mocks.load, saveInternalAccess: mocks.save,
}))
vi.mock('@/api/users', () => ({ usersApi: { listUserGroups: mocks.groups, getAllUsersPage: mocks.users, getUserApiKeys: mocks.keys } }))
vi.mock('@/api/global-models', () => ({ getGlobalModels: mocks.models }))
vi.mock('@/api/access-preview', () => ({ previewAccess: mocks.preview }))
vi.mock('@/components/ui', () => ({
  Dialog: defineComponent({ props: ['modelValue'], setup(props, { slots }) { return () => props.modelValue ? h('div', { role: 'dialog' }, [slots.default?.(), slots.footer?.()]) : null } }),
  Button: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }),
  Input: defineComponent({ props: ['modelValue'], emits: ['update:modelValue'], setup(props, { emit, attrs }) {
    return () => h('input', { ...attrs, value: props.modelValue, onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value) })
  } }),
  Switch: defineComponent({ props: ['modelValue'], emits: ['update:modelValue'], setup(props, { emit }) {
    return () => h('input', { type: 'checkbox', checked: props.modelValue, onChange: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).checked) })
  } }),
}))

let app: App
let root: HTMLDivElement
async function flush() { for (let i = 0; i < 8; i++) { await Promise.resolve(); await nextTick() } }
function input(element: HTMLInputElement, value: string) { element.value = value; element.dispatchEvent(new Event('input', { bubbles: true })) }
function click(label: string) { const buttons = [...root.querySelectorAll('button')]; const button = buttons.find(b => b.textContent?.trim() === label || b.title === label) || buttons.find(b => b.textContent?.includes(label)); expect(button).toBeTruthy(); button!.click() }
async function remount() {
  app.unmount(); root.innerHTML = ''
  app = createApp(InternalAccessPolicies); app.mount(root); await flush()
}
async function select(element: HTMLSelectElement, value: string) {
  element.value = value; element.dispatchEvent(new Event('change', { bubbles: true })); await flush()
}
async function target(type: string, id?: string) {
  root.querySelector<HTMLButtonElement>(`#benefits-target-${type}`)!.click(); await flush()
  if (id) { const button = [...root.querySelectorAll<HTMLButtonElement>('.benefits-subject')].find(button => button.textContent?.includes(id === 'group-1' ? 'Research' : 'User'))!; button.click(); await flush() }
}
function familyConfig(extra = {}) {
  return {
    timezone: 'Asia/Shanghai', groups: {}, users: {},
    model_families: { 'f-glm': { name: 'GLM', models: ['glm', 'glm-pro'] } },
    default: { unlimited: false, monthly_budget_usd: 25, models: {}, families: { 'f-glm': { daily_budget_usd: 2 } } },
    ...extra,
  }
}
async function loadFamilies(extra = {}) {
  mocks.load.mockResolvedValue(familyConfig(extra)); await remount()
}

beforeEach(async () => {
  vi.clearAllMocks()
  mocks.canManage = true
  mocks.load.mockResolvedValue({ timezone: 'Asia/Shanghai', groups: {}, users: {}, default: { unlimited: false, monthly_budget_usd: 25, models: { glm: { five_hour_budget_usd: 10 } } } })
  mocks.save.mockResolvedValue(undefined)
  mocks.groups.mockResolvedValue({ items: [{ id: 'group-1', name: 'Research' }] })
  mocks.users.mockResolvedValue({ items: [{ id: 'user-1', username: 'User' }] })
  mocks.models.mockResolvedValue({ models: [{ name: 'glm' }, { name: 'glm-pro' }, { name: 'gpt' }], total: 3 })
  mocks.keys.mockResolvedValue([])
  mocks.preview.mockResolvedValue(undefined)
  root = document.createElement('div'); document.body.append(root)
  app = createApp(InternalAccessPolicies); app.mount(root); await flush()
})
afterEach(() => { app.unmount(); root.remove() })

describe('internal allocations', () => {
  it('separates three workspaces and retains drafts across page and pool tabs', async () => {
    expect(root.querySelector('#benefits-panel-allocation')?.getAttribute('aria-hidden')).toBe('false')
    expect(root.querySelector('#benefits-panel-families')?.getAttribute('aria-hidden')).toBe('true')
    input(root.querySelector<HTMLInputElement>('[aria-label="每月总金额上限（USD）"]')!, '42')
    root.querySelector<HTMLButtonElement>('#benefits-tab-families')!.click(); await flush()
    expect(root.querySelector('#benefits-panel-allocation')?.getAttribute('aria-hidden')).toBe('true')
    root.querySelector<HTMLButtonElement>('#benefits-tab-preview')!.click(); await flush()
    expect(root.querySelector('#benefits-panel-preview')?.getAttribute('aria-hidden')).toBe('false')
    root.querySelector<HTMLButtonElement>('#benefits-tab-allocation')!.click(); await flush()
    expect(root.querySelector<HTMLInputElement>('[aria-label="每月总金额上限（USD）"]')!.value).toBe('42')
    root.querySelector<HTMLButtonElement>('#benefits-pool-model')!.click(); await flush()
    expect(root.querySelector('#benefits-model-pool')?.getAttribute('aria-hidden')).toBe('false')
    expect(root.querySelector('#benefits-family-pool')?.getAttribute('aria-hidden')).toBe('true')
    click('保存'); await flush(); expect(mocks.save.mock.calls[0]![0].default.monthly_budget_usd).toBe(42)
  })
  it('requires confirmation before refreshing a dirty draft and allows continuing edits', async () => {
    input(root.querySelector<HTMLInputElement>('[aria-label="每月总金额上限（USD）"]')!, '42')
    click('刷新'); await flush(); expect(mocks.load).toHaveBeenCalledTimes(1)
    expect(root.querySelector('[role="dialog"]')).toBeTruthy()
    click('继续编辑'); await flush(); expect(root.querySelector('[role="dialog"]')).toBeNull()
    expect(root.querySelector<HTMLInputElement>('[aria-label="每月总金额上限（USD）"]')!.value).toBe('42')
    click('刷新'); await flush(); click('放弃更改并刷新'); await flush()
    expect(mocks.load).toHaveBeenCalledTimes(2)
    expect(root.querySelector<HTMLInputElement>('[aria-label="每月总金额上限（USD）"]')!.value).toBe('25')
  })
  it('supports keyboard tabs and searches subjects without creating overrides', async () => {
    root.querySelector('#benefits-tab-allocation')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await flush(); expect(root.querySelector('#benefits-tab-families')?.getAttribute('aria-selected')).toBe('true')
    await target('group')
    input(root.querySelector<HTMLInputElement>('[aria-label="搜索分配对象"]')!, 'no-match'); await flush()
    expect(root.querySelectorAll('.benefits-subject')).toHaveLength(0)
    expect(root.textContent).toContain('无匹配对象')
    input(root.querySelector<HTMLInputElement>('[aria-label="搜索分配对象"]')!, 'research'); await flush()
    expect(root.querySelectorAll('.benefits-subject')).toHaveLength(1)
    click('保存'); await flush(); expect(mocks.save.mock.calls[0]![0].groups).toEqual({})
  })
  it('labels each sub-pool amount independently of table layout', () => {
    const table = root.querySelector('table[aria-label="模型子池额度"]')!
    expect(table.querySelector('input[aria-label="单独限额模型"]')).not.toBeNull()
    expect(table.querySelector('input[aria-label="glm 滚动 5 小时额度"]')).not.toBeNull()
    expect(table.querySelector('td[data-label="每日"]')).not.toBeNull()
  })
  it('does not permit changing stale allocations after a failed reload', async () => {
    mocks.load.mockRejectedValue(new Error('offline'))
    click('刷新'); await flush()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('offline')
    expect(root.querySelector<HTMLInputElement>('input[type="number"]')!.disabled).toBe(true)
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
  })
  it('keeps audit allocations readable but rejects editing and saving', async () => {
    app.unmount(); root.innerHTML = ''; mocks.canManage = false
    app = createApp(InternalAccessPolicies); app.mount(root); await flush()
    expect(root.querySelector<HTMLInputElement>('input[type="number"]')!.disabled).toBe(true)
    expect(root.querySelector<HTMLButtonElement>('#benefits-target-group')!.disabled).toBe(false)
    const modelTab = root.querySelector<HTMLButtonElement>('#benefits-pool-model')!
    expect(modelTab.matches(':disabled')).toBe(false)
    modelTab.click(); await flush()
    expect(root.querySelector('#benefits-model-pool')?.getAttribute('aria-hidden')).toBe('false')
    expect(root.querySelector<HTMLInputElement>('input[aria-label="单独限额模型"]')!.disabled).toBe(true)
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
  })
  it('saves fractional money for total windows and model windows without cloning Vue proxies', async () => {
    const fields = root.querySelectorAll<HTMLInputElement>('input[type="number"]')
    input(fields[0]!, '2.5'); input(fields[1]!, '12.75'); input(fields[2]!, '50'); input(fields[4]!, '0.3')
    click('保存'); await flush()
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ default: {
      unlimited: false, five_hour_budget_usd: 2.5, weekly_budget_usd: 12.75, monthly_budget_usd: 50, models: { glm: { five_hour_budget_usd: 10, daily_budget_usd: 0.3 } },
    } }))
    expect(root.textContent).toContain('已保存')
  })
  it('accepts zero total and model monetary limits without removing them', async () => {
    input(root.querySelector<HTMLInputElement>('[aria-label="每月总金额上限（USD）"]')!, '0')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[3]!, '0')
    click('保存'); await flush()
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ default: expect.objectContaining({ monthly_budget_usd: 0, models: { glm: { five_hour_budget_usd: 0 } } }) }))
  })
  it('rejects negative monetary limits', async () => {
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[3]!, '-1')
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(root.textContent).toContain('金额额度须为 0 或有效正数')
  })
  it('explicit unlimited clears all finite quota data', async () => {
    const switchInput = root.querySelector<HTMLInputElement>('input[type="checkbox"]')!
    switchInput.checked = true; switchInput.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    click('保存'); await flush()
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ default: { unlimited: true, models: {} } }))
  })
  it('omitting a money cap removes it without changing model grants', async () => {
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[2]!, '')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[3]!, '')
    click('保存'); await flush()
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ default: {
      unlimited: false, models: { glm: {} },
    } }))
  })
  it('does not save duplicate model names', async () => {
    click('添加子池'); await flush()
    input(root.querySelectorAll<HTMLInputElement>('input[placeholder="模型名称或 *"]')[1]!, 'glm')
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(root.textContent).toContain('模型名称不能为空或重复')
  })
  it('removing all sub-pools leaves total budgets intact and does not modify model permissions', async () => {
    root.querySelector<HTMLButtonElement>('button[title="移除模型子池"]')!.click()
    await flush()
    expect(root.textContent).toContain('无单独子池')
    click('保存'); await flush()
    expect(mocks.save).toHaveBeenCalledWith(expect.objectContaining({ default: {
      unlimited: false, monthly_budget_usd: 25, models: {},
    } }))
    expect(mocks.save.mock.calls[0]![0]).not.toHaveProperty('allowed_models')
  })
  it('reports a rejected write and keeps the draft available', async () => {
    mocks.save.mockRejectedValue(new Error('policy conflict'))
    click('保存'); await flush()
    expect(root.querySelector('[role="alert"]')?.textContent).toContain('policy conflict')
    expect(root.querySelector('input[placeholder="模型名称或 *"]')).toBeTruthy()
  })
  it('creates multiple stable families from exact public catalog names and saves them with allocations', async () => {
    click('添加家族'); await flush()
    const first = root.querySelector<HTMLElement>('[data-family-id]')!
    const id = first.dataset.familyId!
    expect(id).toMatch(/^f-[0-9a-f-]+$/)
    input(first.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!, 'GLM family')
    first.querySelector<HTMLButtonElement>('button')!.click(); await flush()
    const option = [...first.querySelectorAll('span')].find(span => span.textContent === 'glm-pro')!.parentElement!
    const checkbox = option.querySelector<HTMLInputElement>('input')!
    checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    first.querySelector<HTMLButtonElement>('button')!.click(); await flush()
    click('添加家族'); await flush()
    const second = root.querySelectorAll<HTMLElement>('[data-family-id]')[1]!
    input(second.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!, 'Second')
    second.querySelector<HTMLButtonElement>('button')!.click(); await flush()
    const gpt = [...second.querySelectorAll('span')].find(span => span.textContent === 'gpt')!.parentElement!.querySelector<HTMLInputElement>('input')!
    gpt.checked = true; gpt.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    click('添加家族子池'); await flush()
    await select(root.querySelector<HTMLSelectElement>('select[aria-label="限额家族"]')!, id)
    const quotas = root.querySelectorAll<HTMLInputElement>('table[aria-label="家族共享子池额度"] input[type="number"]')
    for (const [index, value] of ['1.1', '2.2', '3.3', '4.4'].entries()) input(quotas[index]!, value)
    click('保存'); await flush()
    const policy = mocks.save.mock.calls[0]![0]
    expect(Object.keys(policy.model_families)).toHaveLength(2)
    expect(policy.model_families[id]).toEqual({ name: 'GLM family', models: ['glm-pro'] })
    expect(policy.default.families[id]).toEqual({ five_hour_budget_usd: 1.1, daily_budget_usd: 2.2, weekly_budget_usd: 3.3, monthly_budget_usd: 4.4 })
    expect(policy).not.toHaveProperty('allowed_models')
  })
  it('keeps renamed families, membership edits and multiple allocation drafts across switches and failed CAS retries', async () => {
    await loadFamilies()
    input(root.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!, 'Renamed')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[2]!, '30')
    await target('group', 'group-1')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[1]!, '8')
    click('添加家族子池'); await flush()
    await select(root.querySelector<HTMLSelectElement>('select[aria-label="限额家族"]')!, 'f-glm')
    input(root.querySelector<HTMLInputElement>('input[aria-label="Renamed 每日额度"]')!, '0.7')
    await target('user', 'user-1')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[0]!, '1.5')
    await target('default')
    expect(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[2]!.value).toBe('30')
    mocks.save.mockRejectedValueOnce(new Error('CAS conflict'))
    click('保存'); await flush()
    expect(root.textContent).toContain('CAS conflict')
    expect(root.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!.value).toBe('Renamed')
    const firstAttempt = mocks.save.mock.calls[0]![0]
    expect(firstAttempt.groups['group-1']).toEqual({ unlimited: false, weekly_budget_usd: 8, models: {}, families: { 'f-glm': { daily_budget_usd: 0.7 } } })
    expect(firstAttempt.users['user-1']).toEqual({ unlimited: false, five_hour_budget_usd: 1.5, models: {} })
    click('保存'); await flush()
    expect(mocks.save.mock.calls[1]![0]).toEqual(firstAttempt)
    expect(root.textContent).toContain('已保存')
  })
  it.each(['default', 'groups', 'users'])('blocks deletion of a family referenced by %s, including unseen subjects', async kind => {
    const allocation = { unlimited: false, models: {}, families: { 'f-glm': {} } }
    await loadFamilies({ default: kind === 'default' ? allocation : null, groups: kind === 'groups' ? { unseen: allocation } : {}, users: kind === 'users' ? { unseen: allocation } : {} })
    const button = root.querySelector<HTMLButtonElement>('button[aria-label="删除家族 GLM"]')!
    expect(button.disabled).toBe(true)
    button.click(); await flush()
    expect(root.querySelector('[data-family-id="f-glm"]')).not.toBeNull()
  })
  it('allows explicit reference removal and family deletion in the same CAS save', async () => {
    await loadFamilies()
    root.querySelector<HTMLButtonElement>('button[title="移除家族子池"]')!.click(); await flush()
    root.querySelector<HTMLButtonElement>('button[aria-label="删除家族 GLM"]')!.click(); await flush()
    click('保存'); await flush()
    const policy = mocks.save.mock.calls[0]![0]
    expect(policy).not.toHaveProperty('model_families')
    expect(policy.default).toEqual({ unlimited: false, monthly_budget_usd: 25, models: {} })
  })
  it('clears model and family sub-pools together for explicit unlimited', async () => {
    await loadFamilies()
    const toggle = root.querySelector<HTMLInputElement>('input[type="checkbox"]')!
    toggle.checked = true; toggle.dispatchEvent(new Event('change', { bubbles: true })); await flush()
    click('保存'); await flush()
    expect(mocks.save.mock.calls[0]![0].default).toEqual({ unlimited: true, models: {} })
    expect(mocks.save.mock.calls[0]![0].model_families).toHaveProperty('f-glm')
  })
  it('accepts an explicit zero family monetary limit', async () => {
    await loadFamilies()
    input(root.querySelector<HTMLInputElement>('input[aria-label="GLM 每日额度"]')!, '0')
    click('保存'); await flush()
    expect(mocks.save.mock.calls[0]![0].default.families['f-glm'].daily_budget_usd).toBe(0)
  })
  it.each(['-1', '0.000000001', '90071993'])('rejects invalid family amounts %s', async value => {
    await loadFamilies()
    input(root.querySelector<HTMLInputElement>('input[aria-label="GLM 每日额度"]')!, value)
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(root.textContent).toContain('金额额度须为 0 或有效正数')
  })
  it('limits combined model and family sub-pools to 15', async () => {
    await loadFamilies({ default: { unlimited: false, models: Object.fromEntries(Array.from({ length: 14 }, (_, i) => [`m${i}`, {}])), families: { 'f-glm': {} } } })
    const add = [...root.querySelectorAll('button')].filter(button => button.textContent?.includes('添加') && button.textContent?.includes('子池'))
    expect(add.every(button => button.disabled)).toBe(true)
    root.querySelector<HTMLButtonElement>('button[title="移除模型子池"]')!.click(); await flush()
    click('添加子池'); await flush()
    input(root.querySelectorAll<HTMLInputElement>('input[aria-label="单独限额模型"]')[13]!, 'new-model')
    click('保存'); await flush()
    const policy = mocks.save.mock.calls[0]![0]
    expect(Object.keys(policy.default.models)).toHaveLength(14)
    expect(Object.keys(policy.default.families)).toHaveLength(1)
  })
  it('validates loaded overflow, duplicate and missing family references when edited', async () => {
    await loadFamilies({ default: { unlimited: false, models: Object.fromEntries(Array.from({ length: 15 }, (_, i) => [`m${i}`, {}])), families: { 'f-glm': {} } } })
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[0]!, '1')
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(root.textContent).toContain('合计最多 15 个')
    await loadFamilies()
    click('添加家族子池'); await flush()
    await select(root.querySelectorAll<HTMLSelectElement>('select[aria-label="限额家族"]')[1]!, 'f-glm')
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(root.textContent).toContain('家族子池不能重复')
  })
  it('rejects empty family names and empty, duplicate or excessive member sets', async () => {
    for (const family of [{ name: ' ', models: ['glm'] }, { name: 'Empty', models: [] }, { name: 'Duplicate', models: ['glm', 'glm'] }, { name: 'Large', models: Array.from({ length: 257 }, (_, i) => `m${i}`) }]) {
      await loadFamilies({ model_families: { 'f-glm': family } })
      click('保存'); await flush()
      expect(mocks.save).not.toHaveBeenCalled()
      expect(root.querySelector('[role="alert"]')).not.toBeNull()
    }
  })
  it('disables additions at 32 families', async () => {
    await loadFamilies({ model_families: Object.fromEntries(Array.from({ length: 32 }, (_, i) => [`f-${i}`, { name: `Family ${i}`, models: ['glm'] }])), default: null })
    const add = [...root.querySelectorAll('button')].find(button => button.textContent?.trim() === '添加家族')!
    expect(add.disabled).toBe(true)
  })
  it('does not create overrides just by browsing subjects and can save definitions without a selected subject', async () => {
    await loadFamilies()
    await target('group', 'group-1')
    await target('user', 'user-1')
    await target('group')
    input(root.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!, 'Renamed')
    click('保存'); await flush()
    expect(mocks.save.mock.calls[0]![0].groups).toEqual({})
    expect(mocks.save.mock.calls[0]![0].users).toEqual({})
    expect(mocks.save.mock.calls[0]![0].model_families['f-glm'].name).toBe('Renamed')
  })
  it('preserves pending personal override removal after a failed save and retries it', async () => {
    await loadFamilies({ users: { 'user-1': { unlimited: false, models: {}, families: { 'f-glm': {} } } } })
    await target('user', 'user-1')
    mocks.save.mockRejectedValueOnce(new Error('offline'))
    click('恢复组权益'); await flush()
    expect(root.textContent).toContain('待移除个人覆盖')
    click('保存'); await flush()
    expect(mocks.save.mock.calls[1]![0].users).toEqual({})
  })
  it('keeps family definitions read-only for audit users', async () => {
    mocks.canManage = false; await loadFamilies()
    expect(root.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!.disabled).toBe(true)
    expect(root.querySelector<HTMLButtonElement>('button[aria-label="删除家族 GLM"]')!.disabled).toBe(true)
    click('保存'); await flush()
    expect(mocks.save).not.toHaveBeenCalled()
  })
  it('loads the full public model catalog page by page using names, not IDs or display names', async () => {
    mocks.models.mockResolvedValueOnce({ models: [{ id: 'upstream-id', name: 'public-a', display_name: 'Friendly' }], total: 2 })
      .mockResolvedValueOnce({ models: [{ name: 'public-b' }], total: 2 })
    await remount()
    expect(mocks.models).toHaveBeenLastCalledWith({ skip: 1, limit: 1000 })
    click('添加家族'); await flush()
    root.querySelector<HTMLElement>('[data-family-id]')!.querySelector<HTMLButtonElement>('button')!.click(); await flush()
    expect(root.textContent).toContain('public-a')
    expect(root.textContent).toContain('public-b')
    expect(root.textContent).not.toContain('Friendly')
    expect(root.textContent).not.toContain('upstream-id')
  })
  it('reports a failed model catalog without blocking existing budget-only edits', async () => {
    mocks.models.mockRejectedValueOnce(new Error('catalog offline'))
    await loadFamilies()
    expect(root.textContent).toContain('catalog offline')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[2]!, '40')
    click('保存'); await flush()
    expect(mocks.save.mock.calls[0]![0].default.monthly_budget_usd).toBe(40)
  })
  it('exposes an isolated validated global draft for preview without saving or clearing edits', async () => {
    await loadFamilies()
    await target('group', 'group-1')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[0]!, '1.2')
    await target('user', 'user-1')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[2]!, '20')
    const buildDraft = app._instance!.exposed!.buildDraft as () => import('@/api/internal-access').InternalAccessConfig
    const policy = buildDraft()
    expect(policy.groups['group-1']!.five_hour_budget_usd).toBe(1.2)
    expect(policy.users['user-1']!.monthly_budget_usd).toBe(20)
    expect(mocks.save).not.toHaveBeenCalled()
    policy.model_families!['f-glm']!.name = 'Not the editor'
    expect(buildDraft().model_families!['f-glm']!.name).toBe('GLM')
    input(root.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!, '')
    expect(buildDraft).toThrow('家族显示名不能为空')
    expect(mocks.save).not.toHaveBeenCalled()
  })
  it('connects preview to the selected user and all unsaved policy changes without writing', async () => {
    await loadFamilies()
    input(root.querySelector<HTMLInputElement>('input[aria-label="家族显示名"]')!, 'Unsaved family')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[2]!, '99')
    await target('user', 'user-1')
    input(root.querySelectorAll<HTMLInputElement>('input[type="number"]')[1]!, '5')
    expect(root.querySelector<HTMLSelectElement>('select[aria-label="预览用户"]')!.value).toBe('user-1')
    click('预览当前草稿'); await flush()
    expect(mocks.preview).toHaveBeenCalledWith('user-1', expect.objectContaining({
      default: expect.objectContaining({ monthly_budget_usd: 99 }),
      users: { 'user-1': { unlimited: false, weekly_budget_usd: 5, models: {} } },
      model_families: { 'f-glm': { name: 'Unsaved family', models: ['glm', 'glm-pro'] } },
    }), undefined, expect.any(AbortSignal))
    expect(mocks.save).not.toHaveBeenCalled()
  })
})
