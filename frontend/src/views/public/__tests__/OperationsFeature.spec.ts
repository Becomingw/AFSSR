import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, nextTick, reactive, type App } from 'vue'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { setI18nLocale } from '@/i18n'
import OperationsFeature from '../OperationsFeature.vue'
import { operationSteps, OPERATIONS_BEAT_MS } from '../operations-example'

const mounts: Array<{ app: App, root: HTMLElement }> = []
function mount(active = false, onLogin = vi.fn()) {
  const props = reactive({ active, onLogin })
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp({ render: () => h(OperationsFeature, props) })
  app.mount(root)
  mounts.push({ app, root })
  return { root, props }
}
function enableMotion(reduced = false) {
  vi.useFakeTimers()
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible')
  vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: reduced, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
}
async function choose(root: HTMLElement, index: number) {
  root.querySelector<HTMLInputElement>(`input[value="${index}"]`)?.click()
  await nextTick()
}
function selected(root: HTMLElement) { return root.querySelector<HTMLInputElement>('input:checked')?.value }
afterEach(() => {
  for (const { app, root } of mounts.splice(0)) { app.unmount(); root.remove() }
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('feedback and Agent operations feature', () => {
  it('shows the feedback lifecycle and its identity, permissions and audit boundaries', () => {
    const { root } = mount()
    expect(root.querySelectorAll('input[type="radio"]')).toHaveLength(4)
    expect(root.querySelector('.arca-operations')?.getAttribute('aria-label')).toContain('不执行真实操作')
    for (const text of ['用户反馈', 'Agent 排查', '授权修复', '验证解决', 'MCP 流程演示', 'API Key 绑定身份', '管理操作遵循权限', '变更过程可审计']) {
      expect(root.textContent).toContain(text)
    }
  })

  it('uses real tool names from the embedded MCP catalog', () => {
    const catalog = readFileSync(resolve(process.cwd(), '../apps/aether-gateway/src/api/mcp/catalog.rs'), 'utf8')
    for (const step of operationSteps) {
      for (const tool of step.tools) {
        if (tool.name === 'afssr_identity') continue
        expect(catalog).toContain(`op!("${tool.name.replace('afssr_', '')}"`)
      }
    }
  })

  it('switches stages locally without making network requests', async () => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)
    const { root } = mount()
    await choose(root, 2)
    expect(root.querySelector('.operations-result')?.textContent).toContain('风险操作先获维护授权')
    expect(root.querySelectorAll('.operations-tool-list li')).toHaveLength(3)
    expect(root.textContent).toContain('预览变更，不执行写入')
    expect(root.querySelector('.arca-operations')?.getAttribute('data-motion')).toBe('paused')
    await choose(root, 3)
    expect(root.querySelector('.operations-result')?.textContent).toContain('验证修复效果后')
    expect(fetch).not.toHaveBeenCalled()
  })

  it('opens the login flow for a guest feedback action', () => {
    const onLogin = vi.fn()
    const { root } = mount(false, onLogin)
    root.querySelector<HTMLButtonElement>('.operations-feedback-link')?.click()
    expect(onLogin).toHaveBeenCalledOnce()
  })

  it('progresses through operation rows then advances to the next stage', async () => {
    enableMotion()
    const { root } = mount(true)
    await nextTick()
    expect(selected(root)).toBe('1')
    expect(root.querySelectorAll('[data-state="complete"]')).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS)
    expect(root.querySelectorAll('[data-state="complete"]')).toHaveLength(2)
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS * 2)
    expect(selected(root)).toBe('2')
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS * 8)
    expect(selected(root)).toBe('0')
  })

  it('pauses on manual selection, resumes, and stops when its section is inactive', async () => {
    enableMotion()
    const { root, props } = mount(true)
    await nextTick()
    await choose(root, 0)
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS * 6)
    expect(selected(root)).toBe('0')
    expect(root.querySelectorAll('[data-state="complete"]')).toHaveLength(0)
    root.querySelector<HTMLButtonElement>('[aria-label="继续演示动画"]')?.click()
    await nextTick()
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS)
    expect(root.querySelectorAll('[data-state="complete"]')).toHaveLength(1)
    props.active = false
    await nextTick()
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS * 5)
    expect(root.querySelectorAll('[data-state="complete"]')).toHaveLength(1)
  })

  it('respects reduced motion and document visibility', async () => {
    enableMotion(true)
    const { root } = mount(true)
    await nextTick()
    expect(root.querySelector<HTMLButtonElement>('.arca-motion-toggle')?.disabled).toBe(true)
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS * 8)
    expect(selected(root)).toBe('1')
  })

  it('stops processing the demonstration when the document is hidden', async () => {
    enableMotion()
    const { root } = mount(true)
    await nextTick()
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden')
    document.dispatchEvent(new Event('visibilitychange'))
    await nextTick()
    await vi.advanceTimersByTimeAsync(OPERATIONS_BEAT_MS * 8)
    expect(selected(root)).toBe('1')
    expect(root.querySelectorAll('[data-state="complete"]')).toHaveLength(1)
  })

  it('supports English copy without exposing translation keys', async () => {
    setI18nLocale('en-US')
    const { root } = mount()
    await choose(root, 2)
    expect(root.querySelector('h2')?.textContent).toContain('Agent-assisted operations')
    expect(root.textContent).toContain('maintenance approval')
    expect(root.textContent).not.toContain('site.home.operations.')
  })
})
