import type { ActivityHeatmap } from '@/types/activity'

const DAY = 86_400_000
export type ActivityMode = 'daily' | 'weekly' | 'cumulative'
export interface TokenDay { date: string; tokens: number; cumulative: number }
export interface TokenWeek { start: string; end: string; tokens: number; days: (TokenDay | null)[]; month: string }
export interface TokenHistory { start: string; end: string; days: TokenDay[]; weeks: TokenWeek[]; total: number; activeDays: number }

function timestamp(date: string): number {
  const value = Date.parse(`${date}T00:00:00Z`)
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || !Number.isFinite(value) || new Date(value).toISOString().slice(0, 10) !== date) throw new Error('Invalid activity date')
  return value
}
const iso = (value: number) => new Date(value).toISOString().slice(0, 10)

export function buildTokenHistory(data: ActivityHeatmap): TokenHistory {
  const start = timestamp(data.start_date), end = timestamp(data.end_date)
  const count = (end - start) / DAY + 1
  if (count < 1 || count > 366 || !Array.isArray(data.days)) throw new Error('Invalid activity range')
  const values = new Map<string, number>()
  for (const day of data.days) {
    const time = timestamp(day.date)
    if (time < start || time > end || values.has(day.date) || !Number.isSafeInteger(day.total_tokens) || day.total_tokens < 0) throw new Error('Invalid activity value')
    values.set(day.date, day.total_tokens)
  }
  let total = 0
  const days = Array.from({ length: count }, (_, index) => {
    const date = iso(start + index * DAY), tokens = values.get(date) ?? 0
    total += tokens
    if (!Number.isSafeInteger(total)) throw new Error('Activity total is too large')
    return { date, tokens, cumulative: total }
  })
  const offset = (new Date(start).getUTCDay() + 6) % 7
  const padded: (TokenDay | null)[] = [...Array<TokenDay | null>(offset).fill(null), ...days]
  while (padded.length % 7) padded.push(null)
  let lastMonth = ''
  const weeks: TokenWeek[] = []
  for (let index = 0; index < padded.length; index += 7) {
    const cells = padded.slice(index, index + 7)
    const valid = cells.filter((day): day is TokenDay => day !== null)
    const first = valid[0], last = valid[valid.length - 1]
    if (!first || !last) continue
    const month = first.date.slice(0, 7)
    weeks.push({ start: first.date, end: last.date, tokens: valid.reduce((sum, day) => sum + day.tokens, 0), days: cells, month: month !== lastMonth ? `${Number(first.date.slice(5, 7))}月` : '' })
    lastMonth = month
  }
  return { start: data.start_date, end: data.end_date, days, weeks, total, activeDays: days.filter(day => day.tokens > 0).length }
}

export function tokenLevel(value: number, maximum: number): number {
  if (value <= 0 || maximum <= 0) return 0
  return Math.min(4, Math.max(1, Math.ceil(Math.sqrt(value / maximum) * 4)))
}
