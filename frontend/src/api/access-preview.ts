import apiClient from './client'
import type { AccountEntitlements } from './account'
import type { InternalAccessConfig, Allocation } from './internal-access'

export interface EntitlementScenario {
  source: 'personal' | 'group' | 'default'
  group_ids: string[]
  allocation: Allocation
  quota: Extract<AccountEntitlements, { enabled: true }>
}
export interface AccessPreview {
  enabled: boolean
  user: { id: string; username: string; is_active: boolean }
  model_access: { source: string; mode: string; allowed_models: string[] | null; allowed_providers: string[] | null; allowed_api_formats: string[] | null }
  key: { id: string; name?: string; currently_usable: boolean; allowed_models: string[] | null; allowed_api_formats: string[] | null } | null
  current: EntitlementScenario
  proposed: EntitlementScenario | null
  changed: boolean
}
export async function previewAccess(user_id: string, config: InternalAccessConfig, api_key_id?: string, signal?: AbortSignal): Promise<AccessPreview> {
  return (await apiClient.post<AccessPreview>('/api/admin/access-preview', { user_id, config, ...(api_key_id ? { api_key_id } : {}) }, { signal })).data
}
