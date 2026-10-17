import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, nextTick, reactive, type App } from 'vue'
import FocusComposer from '../FocusComposer.vue'
import type { FeedbackCategory } from '@/api/feedback'

let app: App
let root: HTMLDivElement
const state = reactive({ open: true, title: 'T', description: 'body', category: 'bug' as FeedbackCategory })
const events = { close: vi.fn(), submit: vi.fn() }
async function flush() { for (let i = 0; i < 8; i++) { await Promise.resolve(); await nextTick() } }
const layer = () => document.body.querySelector<HTMLElement>('.focus-layer')

beforeEach(async () => {
  vi.clearAllMocks()
  Object.assign(state, { open: true, title: 'T', description: 'body', category: 'bug' })
  root = document.createElement('div')
  document.body.append(root)
  app = createApp({
    render: () => h(FocusComposer, {
      open: state.open, title: state.title, description: state.description, category: state.category, busy: false,
      categories: [{ value: 'bug', label: '程序问题' }, { value: 'other', label: '其他' }], issueBase: '/dashboard/feedback', mentionable: ['alice'], error: '',
      onClose: events.close, onSubmit: events.submit,
      'onUpdate:title': (v: string) => { state.title = v }, 'onUpdate:description': (v: string) => { state.description = v },
      'onUpdate:category': (v: FeedbackCategory) => { state.category = v },
    }),
  })
  app.mount(root)
  await flush()
})
afterEach(() => { app.unmount(); root.remove(); document.body.style.overflow = '' })

describe('FocusComposer', () => {
  it('is a full-screen modal that locks page scrolling while open', async () => {
    expect(layer()).not.toBeNull()
    expect(layer()!.getAttribute('aria-modal')).toBe('true')
    expect(document.body.style.overflow).toBe('hidden')
    state.open = false
    await flush()
    expect(layer()).toBeNull()
    expect(document.body.style.overflow).toBe('')
  })

  it('edits the same title, content and category the form holds', async () => {
    const title = layer()!.querySelector<HTMLInputElement>('input.focus-title')!
    title.value = 'New title'
    title.dispatchEvent(new Event('input', { bubbles: true }))
    const text = layer()!.querySelector<HTMLTextAreaElement>('textarea')!
    text.value = 'New body'
    text.dispatchEvent(new Event('input', { bubbles: true }))
    const select = layer()!.querySelector<HTMLSelectElement>('select')!
    select.value = 'other'
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await flush()
    expect(state).toMatchObject({ title: 'New title', description: 'New body', category: 'other' })
  })

  it('Escape leaves focus mode without reaching handlers behind it', async () => {
    const behind = vi.fn()
    window.addEventListener('keydown', behind)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }))
    expect(events.close).toHaveBeenCalledTimes(1)
    expect(behind).not.toHaveBeenCalled()
    window.removeEventListener('keydown', behind)
  })

  it('submits only with a title and content, and can go back to the form', async () => {
    const submit = [...layer()!.querySelectorAll('button')].find(b => b.textContent?.includes('发起主题'))!
    submit.click()
    expect(events.submit).toHaveBeenCalledTimes(1)
    state.description = '   '
    await flush()
    expect(([...layer()!.querySelectorAll('button')].find(b => b.textContent?.includes('发起主题')))!.disabled).toBe(true)
    ;[...layer()!.querySelectorAll('button')].find(b => b.textContent?.includes('返回表单'))!.click()
    expect(events.close).toHaveBeenCalled()
  })
})
