import { describe, expect, it } from 'vitest'
import { buildTokenHistory, tokenLevel } from '../tokenActivity'
import type { ActivityHeatmap } from '@/types/activity'

const fixture = (): ActivityHeatmap => ({ start_date: '2026-10-10', end_date: '2026-10-16', total_days: 7, max_requests: 999, days: [
  { date: '2026-10-11', requests: 999, total_tokens: 10, total_cost: 0 },
  { date: '2026-10-12', requests: 1, total_tokens: 20, total_cost: 0 },
  { date: '2026-10-15', requests: 1, total_tokens: 70, total_cost: 0 },
] })
describe('Token activity calendar', () => {
  it('fills missing dates, uses Monday weeks, clips partial weeks and accumulates tokens, not calls', () => {
    const value = buildTokenHistory(fixture())
    expect(value.days).toHaveLength(7)
    expect(value.days.map(day => day.tokens)).toEqual([0, 10, 20, 0, 0, 70, 0])
    expect(value.days.map(day => day.cumulative)).toEqual([0, 10, 30, 30, 30, 100, 100])
    expect(value.weeks[0]?.days.slice(0, 5)).toEqual(Array(5).fill(null))
    expect(value.weeks.map(week => [week.start, week.end, week.tokens])).toEqual([['2026-10-10', '2026-10-11', 10], ['2026-10-12', '2026-10-16', 90]])
    expect(value.total).toBe(100); expect(value.activeDays).toBe(3)
  })
  it('keeps UTC calendar dates correct through leap days and year changes', () => {
    const base = { ...fixture(), days: [] }
    expect(buildTokenHistory({ ...base, start_date: '2024-02-28', end_date: '2024-03-01' }).days.map(day => day.date)).toEqual(['2024-02-28', '2024-02-29', '2024-03-01'])
    const fullYear = buildTokenHistory({ ...base, start_date: '2025-10-17', end_date: '2026-10-16' })
    expect(fullYear.days).toHaveLength(365); expect(fullYear.weeks.length).toBeLessThanOrEqual(54)
    expect(fullYear.days.every(day => day.tokens === 0)).toBe(true)
    expect(fullYear.weeks.filter(week => week.month).length).toBeGreaterThanOrEqual(12)
  })
  it('rejects malformed data rather than showing fabricated zero usage', () => {
    for (const change of [{ start_date: '2026-02-30' }, { end_date: '2026-10-01' }, { end_date: '2028-10-16' }, { days: [{ ...fixture().days[0]!, total_tokens: -1 }] }, { days: [{ ...fixture().days[0]!, total_tokens: NaN }] }, { days: [...fixture().days, fixture().days[0]!] }]) expect(() => buildTokenHistory({ ...fixture(), ...change })).toThrow()
  })
  it('uses a neutral zero level and four positive intensity levels', () => {
    expect(tokenLevel(0, 100)).toBe(0); expect(tokenLevel(1, 100)).toBe(1)
    expect(tokenLevel(25, 100)).toBe(2); expect(tokenLevel(50, 100)).toBe(3)
    expect(tokenLevel(100, 100)).toBe(4); expect(tokenLevel(200, 100)).toBe(4)
    expect(tokenLevel(0, 0)).toBe(0)
  })
})
