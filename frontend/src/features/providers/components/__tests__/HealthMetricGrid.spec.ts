import { createApp, h } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import { createI18n } from '@/i18n'
import HealthMetricGrid from '../HealthMetricGrid.vue'

const cleanups: Array<() => void> = []
function mount(props: InstanceType<typeof HealthMetricGrid>['$props']) {
  const root = document.createElement('div')
  document.body.appendChild(root)
  const app = createApp({ render: () => h(HealthMetricGrid, props) })
  app.use(createI18n())
  app.mount(root)
  cleanups.push(() => { app.unmount(); root.remove() })
  return root.firstElementChild as HTMLElement
}
afterEach(() => { cleanups.splice(0).forEach(cleanup => cleanup()) })

describe('HealthMetricGrid', () => {
  it('preserves complete metrics and lets narrow containers shrink safely', () => {
    const root = mount({
        totalAttempts: 100,
        successRate: 1,
        avgLatencyMs: 123456789,
        avgFirstByteMs: 9876543,
        avgTps: 1234567,
    })
    expect(root.textContent).toContain('100.00%')
    expect(root.classList.contains('min-w-0')).toBe(true)
    expect(root.classList.contains('sm:grid-cols-4')).toBe(false)
    const values = root.querySelectorAll('.tabular-nums')
    expect(values.length).toBe(4)
    for (const value of values) {
      expect(value.classList.contains('[overflow-wrap:anywhere]')).toBe(true)
      expect(value.parentElement?.classList.contains('min-w-0')).toBe(true)
    }
  })

  it('keeps the empty availability state readable', () => {
    const root = mount({ totalAttempts: 0, successRate: 0 })
    expect(root.querySelectorAll('.tabular-nums').length).toBe(4)
    expect(root.textContent).toContain('可用率')
  })
})
