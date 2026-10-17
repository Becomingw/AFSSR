import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp, h, nextTick, ref, type App } from 'vue'
import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import HarnessCarousel from '../HarnessCarousel.vue'
import { harnesses, HARNESS_ROTATION_MS, wrapHarnessIndex } from '../harnesses'

const mounts: Array<{ app: App, root: HTMLElement }> = []
let motionMatches = false
const motionListeners = new Set<() => void>()

beforeEach(() => {
  vi.useFakeTimers()
  vi.spyOn(document, 'hidden', 'get').mockReturnValue(false)
  motionMatches = false
  motionListeners.clear()
  vi.stubGlobal('matchMedia', vi.fn(() => ({
    get matches() { return motionMatches },
    addEventListener: (_: string, listener: () => void) => motionListeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => motionListeners.delete(listener),
  })))
})

afterEach(() => {
  for (const { app, root } of mounts.splice(0)) {
    app.unmount()
    root.remove()
  }
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

function mount() {
  const active = ref(true)
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp({ render: () => h(HarnessCarousel, { active: active.value }) })
  app.mount(root)
  mounts.push({ app, root })
  return { root, active, app }
}

async function settle() {
  await nextTick()
  await vi.advanceTimersByTimeAsync(80)
  await nextTick()
}

function selected(root: HTMLElement) {
  return [...root.querySelectorAll('.harness-pagination button')].findIndex(button => button.getAttribute('aria-pressed') === 'true')
}

function click(root: HTMLElement, selector: string) {
  const button = root.querySelector<HTMLButtonElement>(selector)
  expect(button).not.toBeNull()
  button?.click()
}

describe('HarnessCarousel', () => {
  it('shows only the active Harness and its two neighbors, rather than every logo', () => {
    const { root } = mount()
    expect(selected(root)).toBe(1)
    expect(root.querySelectorAll('img')).toHaveLength(3)
    expect(root.querySelector('h2')?.textContent).toBe('Claude Code')
    expect(root.querySelectorAll('.harness-pagination button')).toHaveLength(harnesses.length)
  })

  it('rotates automatically and does not announce every automatic change', async () => {
    const { root } = mount()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS)
    await settle()
    expect(selected(root)).toBe(2)
    expect(root.querySelector('.harness-current')?.getAttribute('aria-live')).toBe('off')
  })

  it('supports previous/next with wraparound and pauses after manual selection', async () => {
    const { root } = mount()
    click(root, '[aria-label="上一个 Harness"]')
    await settle()
    expect(selected(root)).toBe(0)
    click(root, '[aria-label="上一个 Harness"]')
    await settle()
    expect(selected(root)).toBe(4)
    click(root, '[aria-label="下一个 Harness"]')
    await settle()
    expect(selected(root)).toBe(0)
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS * 2)
    expect(selected(root)).toBe(0)
  })

  it('can pause and resume automatic rotation', async () => {
    const { root } = mount()
    click(root, '[aria-label="暂停自动轮播"]')
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS * 2)
    expect(selected(root)).toBe(1)
    click(root, '[aria-label="继续自动轮播"]')
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS)
    expect(selected(root)).toBe(2)
  })

  it('supports keyboard arrow navigation without requiring pointer interaction', async () => {
    const { root } = mount()
    root.querySelector('.harness-carousel')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await settle()
    expect(selected(root)).toBe(2)
    root.querySelector('.harness-carousel')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }))
    await settle()
    expect(selected(root)).toBe(1)
  })

  it('keeps every Harness reachable with an appropriate animation and local asset', async () => {
    const { root } = mount()
    for (const [index, harness] of harnesses.entries()) {
      root.querySelectorAll<HTMLButtonElement>('.harness-pagination button')[index]?.click()
      await settle()
      expect(selected(root)).toBe(index)
      expect(root.querySelector('h2')?.textContent).toBe(harness.name)
      expect(root.querySelector(`.motion-${harness.id} img`)?.getAttribute('src')).toBe(harness.logo)
      expect(existsSync(resolve(process.cwd(), `public${harness.logo}`))).toBe(true)
    }
  })

  it('does not rotate while the user is hovering over the stage', async () => {
    const { root } = mount()
    root.querySelector('.harness-stage')?.dispatchEvent(new Event('pointerenter'))
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS * 2)
    expect(selected(root)).toBe(1)
    root.querySelector('.harness-stage')?.dispatchEvent(new Event('pointerleave'))
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS)
    expect(selected(root)).toBe(2)
  })

  it('stops offscreen and resumes only when the first page is active', async () => {
    const { root, active } = mount()
    active.value = false
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS * 2)
    expect(selected(root)).toBe(1)
    active.value = true
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS)
    expect(selected(root)).toBe(2)
  })

  it('stops when the browser tab is hidden', async () => {
    const { root } = mount()
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(true)
    document.dispatchEvent(new Event('visibilitychange'))
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS * 2)
    expect(selected(root)).toBe(1)
  })

  it('respects reduced motion on initial load and still allows manual selection', async () => {
    motionMatches = true
    const { root } = mount()
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS * 2)
    expect(selected(root)).toBe(1)
    expect(root.querySelector<HTMLButtonElement>('[aria-label="继续自动轮播"]')?.disabled).toBe(true)
    click(root, '[aria-label="下一个 Harness"]')
    await settle()
    expect(selected(root)).toBe(2)
  })

  it('stops rotation if reduced motion is enabled while mounted', async () => {
    const { root } = mount()
    motionMatches = true
    for (const listener of motionListeners) listener()
    await settle()
    await vi.advanceTimersByTimeAsync(HARNESS_ROTATION_MS * 2)
    expect(selected(root)).toBe(1)
  })

  it('cleans up rotation timers and media listeners on unmount', () => {
    const { app } = mount()
    expect(vi.getTimerCount()).toBe(1)
    expect(motionListeners.size).toBe(1)
    app.unmount()
    mounts.splice(0)
    expect(vi.getTimerCount()).toBe(0)
    expect(motionListeners.size).toBe(0)
  })

  it('wraps both negative and positive indexes', () => {
    expect(wrapHarnessIndex(-1)).toBe(harnesses.length - 1)
    expect(wrapHarnessIndex(harnesses.length)).toBe(0)
  })
})
