import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import UserFormDialog from '../UserFormDialog.vue'

const mocks = vi.hoisted(() => ({ models: vi.fn(), config: vi.fn() }))
vi.mock('@/api/global-models', () => ({ getGlobalModels: mocks.models }))
vi.mock('@/api/admin', () => ({ adminApi: { getSystemConfig: mocks.config } }))
vi.mock('@/i18n', async importOriginal => ({ ...await importOriginal<typeof import('@/i18n')>(), useI18n: () => ({ legacyT: (value: string) => value }) }))
vi.mock('@/components/common', () => ({ MultiSelect: defineComponent({ props: ['modelValue', 'options'], setup(props) { return () => h('div', { 'data-model-selection': true }, JSON.stringify(props.modelValue)) } }) }))
vi.mock('@/components/ui', () => {
  const slot = defineComponent({ setup(_, { slots }) { return () => h('div', slots.default?.()) } })
  return {
    Dialog: defineComponent({ props: ['modelValue'], setup(props, { slots }) { return () => props.modelValue ? h('div', [slots.header?.(), slots.default?.(), slots.footer?.()]) : null } }),
    Button: defineComponent({ setup(_, { slots, attrs }) { return () => h('button', attrs, slots.default?.()) } }),
    Input: defineComponent({ props: ['modelValue'], emits: ['update:modelValue'], setup(props, { attrs, emit }) { return () => h('input', { ...attrs, value: props.modelValue, onInput: (event: Event) => emit('update:modelValue', (event.target as HTMLInputElement).value) }) } }),
    Label: slot, Switch: slot, Select: slot, SelectTrigger: slot, SelectValue: slot, SelectContent: slot, SelectItem: slot,
  }
})

let app: App
let root: HTMLDivElement
let emitted: Record<string, unknown>[]
async function flush() { for (let i = 0; i < 8; i++) { await Promise.resolve(); await nextTick() } }
async function mount(models: string[] | null = ['glm']) {
  const open = ref(false)
  root = document.createElement('div'); document.body.append(root)
  app = createApp({ setup() { return () => h(UserFormDialog, { open: open.value, user: {
    id: 'user-fixture', username: 'model-user', email: '', role: 'user', allowed_models: models,
    allowed_models_mode: models === null ? 'unrestricted' : models.length ? 'specific' : 'deny_all',
  }, onSubmit: data => { emitted.push({ ...data }) } }) } })
  app.mount(root); open.value = true; await flush()
}
async function mode(value: string) {
  const select = root.querySelector<HTMLSelectElement>('#user-model-policy')!
  select.value = value; select.dispatchEvent(new Event('change', { bubbles: true })); await flush()
}
function submit() { [...root.querySelectorAll('button')].find(button => button.textContent?.trim() === '更新')!.click() }

beforeEach(() => {
  vi.clearAllMocks(); emitted = []
  mocks.models.mockResolvedValue({ models: [{ name: 'glm' }, { name: 'fabel' }] })
  mocks.config.mockResolvedValue({ value: 'weak' })
})
afterEach(() => { app?.unmount(); root?.remove() })

describe('personal model permissions', () => {
  it('saves a specific model list in the user payload, independent of money pools', async () => {
    await mount(); submit(); await flush()
    expect(emitted[0]).toMatchObject({ id: 'user-fixture', allowed_models: ['glm'] })
    expect(emitted[0]).not.toHaveProperty('models')
    expect(emitted[0]).not.toHaveProperty('monthly_budget_usd')
  })
  it('null restores group model permissions', async () => {
    await mount(); await mode('inherit'); submit(); await flush()
    expect(emitted[0]).toMatchObject({ allowed_models: null })
  })
  it('an explicit empty list denies model use rather than allowing everything', async () => {
    await mount(); await mode('deny_all'); submit(); await flush()
    expect(emitted[0]).toMatchObject({ allowed_models: [] })
  })
  it('does not submit an empty specific selection', async () => {
    await mount(null); await mode('specific'); submit(); await flush()
    expect(emitted).toEqual([])
  })
  it('blocks permission saves when the catalog cannot be loaded', async () => {
    mocks.models.mockRejectedValue(new Error('catalog unavailable'))
    await mount(); submit(); await flush()
    expect(root.textContent).toContain('catalog unavailable')
    expect(emitted).toEqual([])
  })
})
