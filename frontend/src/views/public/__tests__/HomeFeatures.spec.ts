import { afterEach, describe, expect, it, vi } from 'vitest'
import { createApp, nextTick, type App, type Component } from 'vue'
import { existsSync } from 'node:fs'
import { resolve } from 'node:path'
import { setI18nLocale } from '@/i18n'
import ProtocolFeature from '../ProtocolFeature.vue'
import EntitlementsFeature from '../EntitlementsFeature.vue'
import { exampleProfiles, exampleWindows, getAnimatedEntitlementExample, getEntitlementExample, protocolExamples, quotaPercent, USAGE_ANIMATION_MS, USAGE_RISE_MS } from '../feature-examples'

const mounts: Array<{ app: App, root: HTMLElement }> = []
function mount(component: Component, props = {}) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp(component, props)
  app.mount(root)
  mounts.push({ app, root })
  return root
}
async function choose(root: HTMLElement, name: string, value: string) {
  const input = root.querySelector<HTMLInputElement>(`input[name="${name}"][value="${value}"]`)
  expect(input).not.toBeNull()
  input?.click()
  await nextTick()
}
afterEach(() => {
  for (const { app, root } of mounts.splice(0)) {
    app.unmount()
    root.remove()
  }
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('protocol feature illustration', () => {
  it('shows four formats including Gemini and removes the footer disclaimer', () => {
    const root = mount(ProtocolFeature)
    expect(root.querySelectorAll('input[type="radio"]')).toHaveLength(4)
    expect(root.querySelector('input[value="gemini"]')?.getAttribute('aria-label')).toContain('Gemini')
    expect(root.querySelector('.protocol-endpoints')?.textContent).toContain('/v1/responses')
    expect(root.querySelector('.protocol-endpoints')?.textContent).toContain('/v1/messages')
    expect(root.textContent).toContain('流式响应')
    expect(root.textContent).toContain('工具调用')
    expect(root.textContent).toContain('多轮工具结果回传')
    expect(root.querySelector('.arca-feature-footnote')).toBeNull()
  })

  it.each(protocolExamples)('updates the illustrative upstream for $id', async (protocol) => {
    const root = mount(ProtocolFeature)
    await choose(root, 'home-protocol', protocol.id)
    expect(root.querySelector('.protocol-upstream')?.textContent).toContain(protocol.targetFormat)
    expect(root.querySelector('.protocol-upstream img')?.getAttribute('src')).toBe(protocol.targetIcon)
    expect(root.querySelector('.protocol-endpoints')?.textContent).toContain(protocol.endpoint)
    expect(root.querySelector('.protocol-endpoints')?.textContent).toContain(protocol.targetEndpoint)
    expect(existsSync(resolve(process.cwd(), `public${protocol.targetIcon}`))).toBe(true)
  })

  it('pauses its packet animation when the page is inactive', () => {
    const root = mount(ProtocolFeature, { active: false })
    expect(root.querySelector('.arca-protocol')?.getAttribute('data-active')).toBe('false')
  })
})

describe('entitlement feature illustration', () => {
  it('uses Astra and the requested concise copy without explanatory small print', () => {
    const root = mount(EntitlementsFeature)
    expect(root.querySelector('.arca-entitlements')?.getAttribute('aria-label')).toContain('非账户数据')
    for (const text of ['权益示例', '示例配置', '子池消耗同时计入总额度', '未纳入子池', '个人配置优先于分组', 'GLM']) {
      expect(root.textContent).not.toContain(text)
    }
    expect(root.textContent).toContain('用户与分组配置联合')
    expect(root.textContent).toContain('Astra 家族')
    expect(root.textContent).toContain('模型权限独立设置')
    expect(root.querySelectorAll('[role="progressbar"]')).toHaveLength(3)
    expect(root.querySelectorAll('input[name="home-window"]')).toHaveLength(3)
  })

  it('switches between the actual supported total budget windows', async () => {
    const root = mount(EntitlementsFeature)
    for (const window of exampleWindows) {
      await choose(root, 'home-window', window)
      const example = getEntitlementExample('team', window)
      expect(root.querySelector('.entitlements-meter')?.getAttribute('aria-valuemax')).toBe(String(example.limit))
      expect(root.querySelector('.entitlements-meter')?.getAttribute('aria-valuenow')).toBe(String(example.used))
    }
    expect(root.querySelector('input[value="daily"]')).toBeNull()
  })

  it('shows the personal override rather than enforcing the lower group amount', async () => {
    const root = mount(EntitlementsFeature)
    await choose(root, 'home-profile', 'special')
    expect(root.querySelector('.entitlements-meter')?.getAttribute('aria-valuemax')).toBe('40')
    expect(root.querySelector('.entitlements-policy-source')).toBeNull()
    await choose(root, 'home-window', 'monthly')
    expect(root.querySelector('.entitlements-meter')?.getAttribute('aria-valuemax')).toBe('1200')
  })

  it('does not represent unlimited access as 100% budget usage', async () => {
    const root = mount(EntitlementsFeature)
    await choose(root, 'home-profile', 'unlimited')
    expect(root.querySelector('.entitlements-amount')?.textContent).toContain('不限额')
    expect(root.querySelectorAll('[role="progressbar"]')).toHaveLength(0)
    expect(root.textContent).not.toContain('100%')
    await choose(root, 'home-profile', 'team')
    expect(root.querySelectorAll('[role="progressbar"]')).toHaveLength(3)
  })

  it('keeps subpool spending included in overall spending for every example', () => {
    for (const profile of exampleProfiles) {
      for (const window of exampleWindows) {
        const example = getEntitlementExample(profile, window)
        expect(example.otherUsed).toBeGreaterThanOrEqual(0)
        expect(example.families.reduce((sum, family) => sum + family.used, example.otherUsed)).toBe(example.used)
        if (example.limit !== null) expect(example.used).toBeLessThanOrEqual(example.limit)
      }
    }
  })

  it('clamps bar widths to valid bounds', () => {
    expect(quotaPercent(6, 20)).toBe(30)
    expect(quotaPercent(30, 20)).toBe(100)
    expect(quotaPercent(-2, 20)).toBe(0)
    expect(quotaPercent(6, 0)).toBe(0)
  })

  it('localizes feature headings and controls to English', async () => {
    setI18nLocale('en-US')
    const protocol = mount(ProtocolFeature)
    const entitlements = mount(EntitlementsFeature)
    await nextTick()
    expect(protocol.querySelector('h2')?.textContent).toBe('Multiple protocols. One gateway.')
    expect(entitlements.querySelector('h2')?.textContent).toBe('The right access for every person.')
    expect(entitlements.textContent).toContain('Combined user and group configuration')
    expect(entitlements.querySelector('.arca-feature-footnote')).toBeNull()
  })

  it('animates consumption and recovery while keeping all subpools inside total usage', () => {
    for (const profile of exampleProfiles) {
      for (const window of exampleWindows) {
        const start = getAnimatedEntitlementExample(profile, window, 0)
        const peak = getAnimatedEntitlementExample(profile, window, USAGE_RISE_MS)
        const restored = getAnimatedEntitlementExample(profile, window, USAGE_ANIMATION_MS)
        expect(peak.used).toBeGreaterThanOrEqual(start.used)
        expect(restored.used).toBe(start.used)
        for (let elapsed = 0; elapsed <= USAGE_ANIMATION_MS; elapsed += 1000) {
          const sample = getAnimatedEntitlementExample(profile, window, elapsed)
          const familyTotal = sample.families.reduce((sum, family) => sum + family.used, 0)
          expect(familyTotal + sample.otherUsed).toBeCloseTo(sample.used, 2)
          if (sample.limit !== null) expect(sample.used).toBeLessThanOrEqual(sample.limit)
          for (const family of sample.families) {
            if (family.limit !== null) expect(family.used).toBeLessThanOrEqual(family.limit)
          }
        }
      }
    }
  })

  it('rises quickly in one second then drains slowly over eleven seconds', () => {
    const usage = (elapsed: number) => getAnimatedEntitlementExample('team', 'five_hour', elapsed).used
    const baseline = usage(0)
    const peak = usage(USAGE_RISE_MS)
    expect(USAGE_RISE_MS).toBe(1000)
    expect(USAGE_ANIMATION_MS - USAGE_RISE_MS).toBe(11000)
    expect(peak).toBeGreaterThan(baseline)
    expect(usage(USAGE_ANIMATION_MS)).toBe(baseline)
    const descentMidpoint = USAGE_RISE_MS + (USAGE_ANIMATION_MS - USAGE_RISE_MS) / 2
    const slowChange = usage(descentMidpoint - 250) - usage(descentMidpoint + 250)
    const fastChange = usage(500) - baseline
    expect(fastChange).toBeGreaterThan(slowChange * 5)
    expect(usage(500)).toBeCloseTo((baseline + peak) / 2, 1)
    for (let elapsed = USAGE_RISE_MS; elapsed < USAGE_ANIMATION_MS; elapsed += 500) {
      expect(usage(elapsed + 500)).toBeLessThanOrEqual(usage(elapsed))
    }
  })
})

function enableMotion(reduced = false) {
  vi.useFakeTimers()
  vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible')
  vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: reduced, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
}

function totalUsed(root: HTMLElement) {
  return Number(root.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow'))
}

describe('feature motion lifecycle', () => {
  it('advances numbers and supports pause and resume', async () => {
    enableMotion()
    const root = mount(EntitlementsFeature, { active: true })
    await nextTick()
    await vi.advanceTimersByTimeAsync(3000)
    expect(totalUsed(root)).toBeGreaterThan(6)
    root.querySelector<HTMLButtonElement>('[aria-label="暂停演示动画"]')?.click()
    await nextTick()
    const pausedUsage = totalUsed(root)
    await vi.advanceTimersByTimeAsync(3000)
    expect(totalUsed(root)).toBe(pausedUsage)
    root.querySelector<HTMLButtonElement>('[aria-label="继续演示动画"]')?.click()
    await nextTick()
    await vi.advanceTimersByTimeAsync(1500)
    expect(totalUsed(root)).toBeLessThan(pausedUsage)
  })

  it('does not animate when the section is inactive or reduced motion is enabled', async () => {
    enableMotion()
    const inactive = mount(EntitlementsFeature, { active: false })
    await nextTick()
    await vi.advanceTimersByTimeAsync(3000)
    expect(totalUsed(inactive)).toBe(6)
    expect(inactive.querySelector('.arca-entitlements')?.getAttribute('data-motion')).toBe('paused')
  })

  it('respects reduced motion even on the active section', async () => {
    enableMotion(true)
    const root = mount(EntitlementsFeature, { active: true })
    await nextTick()
    await vi.advanceTimersByTimeAsync(3000)
    expect(totalUsed(root)).toBe(6)
    expect(root.querySelector<HTMLButtonElement>('.arca-motion-toggle')?.disabled).toBe(true)
  })

  it('stops when the document becomes hidden', async () => {
    enableMotion()
    const root = mount(EntitlementsFeature, { active: true })
    await nextTick()
    await vi.advanceTimersByTimeAsync(2500)
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden')
    document.dispatchEvent(new Event('visibilitychange'))
    await nextTick()
    const pausedUsage = totalUsed(root)
    await vi.advanceTimersByTimeAsync(3000)
    expect(totalUsed(root)).toBe(pausedUsage)
  })
})
