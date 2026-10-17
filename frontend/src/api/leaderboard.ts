import apiClient from './client'

export interface RankEntry { rank: number; id: string; name: string; value: number | null }
export interface RankBoard { key: string; unit: string; items: RankEntry[] }
export interface LeaderboardReport {
  masked: boolean
  users: number
  models: number
  requests: number
  tokens: number
  cost: number | null
  boards: RankBoard[]
  hours: Array<{ hour: number; requests: number; leaders: RankEntry[] }>
  early_birds: Array<{ date: string; winners: RankEntry[]; time: string | null }>
  night_owls: Array<{ date: string; winners: RankEntry[] }>
  start_date: string
  end_date: string
  tz_offset_minutes: number
  generated_at: number
  activity_observed_from: number | null
}

export async function getLeaderboard(admin: boolean, preset: string, offset: number, signal?: AbortSignal) {
  const { data } = await apiClient.get<LeaderboardReport>(admin ? '/api/admin/stats/leaderboard/overview' : '/api/leaderboard', {
    params: { preset, tz_offset_minutes: offset }, signal,
  })
  return data
}
