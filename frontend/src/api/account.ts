import apiClient from './client'

export interface AccountWindow {
  model: string | null
  family?: string | null
  family_name?: string | null
  models?: string[] | null
  period: 'five_hour' | 'daily' | 'weekly' | 'monthly'
  limit_usd: number | null
  used_usd: number
  reserved_usd: number
  remaining_usd: number | null
  starts_at: number
  ends_at: number
  resets_at: number | null
}
export type AccountEntitlements = { enabled: false } | {
  enabled: true
  generated_at: number
  timezone: string
  source: 'personal' | 'inherited'
  unlimited: boolean
  windows: AccountWindow[]
}
export const accountApi = {
  async entitlements(signal?: AbortSignal): Promise<AccountEntitlements> {
    return (await apiClient.get<AccountEntitlements>('/api/users/me/entitlements', { signal })).data
  },
}
