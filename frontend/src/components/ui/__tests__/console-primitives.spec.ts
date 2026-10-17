import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, type App } from 'vue'
import Button from '../button.vue'
import Badge from '../badge.vue'

let app: App | undefined
let host: HTMLDivElement | undefined

afterEach(() => { app?.unmount(); host?.remove() })

function render(component: typeof Button | typeof Badge, props: Record<string, unknown>) {
  host = document.createElement('div')
  document.body.append(host)
  app = createApp({ render: () => h(component, props, () => 'Test') })
  app.mount(host)
  return host.firstElementChild as HTMLElement
}

describe('console primitive styling hooks', () => {
  it('preserves button semantics and events with variant hooks', () => {
    const onClick = vi.fn()
    const button = render(Button, { variant: 'outline', type: 'submit', 'aria-label': 'Save', onClick }) as HTMLButtonElement
    expect(button.dataset.ui).toBe('button')
    expect(button.dataset.variant).toBe('outline')
    expect(button.type).toBe('submit')
    expect(button.getAttribute('aria-label')).toBe('Save')
    button.click()
    expect(onClick).toHaveBeenCalledOnce()
  })

  it('keeps disabled default commands inert', () => {
    const onClick = vi.fn()
    const button = render(Button, { disabled: true, onClick }) as HTMLButtonElement
    expect(button.dataset.variant).toBe('default')
    expect(button.disabled).toBe(true)
    button.click()
    expect(onClick).not.toHaveBeenCalled()
  })

  it.each(['success', 'warning', 'destructive', 'outline'])('exposes %s badge styling without losing content', variant => {
    const badge = render(Badge, { variant, title: 'Status' })
    expect(badge.dataset.ui).toBe('badge')
    expect(badge.dataset.variant).toBe(variant)
    expect(badge.textContent).toBe('Test')
    expect(badge.title).toBe('Status')
  })
})
