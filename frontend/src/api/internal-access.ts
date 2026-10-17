import { adminApi } from './admin'
import apiClient from './client'
import { getErrorStatus } from '@/types/api-error'

export const INTERNAL_ACCESS_KEY = 'internal_access_policy'
let expectedValue: unknown = null
export interface ModelAllowance {
  five_hour_budget_usd?: number
  daily_budget_usd?: number
  weekly_budget_usd?: number
  monthly_budget_usd?: number
}
export interface ModelFamily {
  name: string
  models: string[]
}
export interface Allocation {
  unlimited: boolean
  five_hour_budget_usd?: number
  weekly_budget_usd?: number
  monthly_budget_usd?: number
  models: Record<string, ModelAllowance>
  families?: Record<string, ModelAllowance>
}
export interface InternalAccessConfig {
  timezone: string
  default?: Allocation | null
  groups: Record<string, Allocation>
  users: Record<string, Allocation>
  model_families?: Record<string, ModelFamily>
}
export function emptyConfig(): InternalAccessConfig {
  return { timezone: 'Asia/Shanghai', groups: {}, users: {} }
}
export function emptyAllocation(): Allocation {
  return { unlimited: false, models: {} }
}
export async function loadInternalAccess(): Promise<InternalAccessConfig> {
  let value: unknown
  try {
    value = (await adminApi.getSystemConfig(INTERNAL_ACCESS_KEY)).value
  } catch (error) {
    if (getErrorStatus(error) !== 404) throw error
    expectedValue = null
    return emptyConfig()
  }
  expectedValue = value == null ? null : JSON.parse(JSON.stringify(value))
  if (value != null) {
    const config = value as InternalAccessConfig
    const allocations = [config.default, ...Object.values(config.groups ?? {}), ...Object.values(config.users ?? {})]
    const legacy = ['five_hour_requests', 'daily_requests', 'weekly_requests', 'monthly_requests']
    if (allocations.some(allocation => Object.values(allocation?.models ?? {}).some(model =>
      legacy.some(key => Object.prototype.hasOwnProperty.call(model, key))))) {
      throw new Error('旧次数额度不能自动换算为金额，请先由管理员明确重新分配金额额度')
    }
  }
  return value == null ? emptyConfig() : value as InternalAccessConfig
}
export async function saveInternalAccess(config: InternalAccessConfig): Promise<void> {
  await apiClient.put(`/api/admin/system/configs/${INTERNAL_ACCESS_KEY}`, {
    value: config, expected_value: expectedValue, description: 'Internal total budgets and optional model sub-pools',
  })
  expectedValue = JSON.parse(JSON.stringify(config))
}
