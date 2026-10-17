import apiClient from './client'

export type CompatibilityClient = 'codex' | 'claude_code' | 'open_code' | 'generic'
export type CompatibilityProtocol = 'openai:responses' | 'openai:chat' | 'claude:messages'
export interface CompatibilityConfig {
  client: CompatibilityClient
  client_version: string
  client_protocol: CompatibilityProtocol
  model: string
  upstream_format: CompatibilityProtocol
  mapped_model: string
  channel?: { provider_id: string; provider_model_id: string; endpoint_id: string }
}
export interface CompatibilityProfile { id: string; config: CompatibilityConfig; config_fingerprint: string }
export interface CompatibilityCase { case: string; status: string; reason: string }
export interface CompatibilityRun {
  id: string; profile_id: string; config: CompatibilityConfig; config_fingerprint: string
  suite: string; evidence: string; created_at: number; cases: CompatibilityCase[]
}
export interface CompatibilitySnapshot {
  schema_version: number; revision: number; can_run: boolean; suite: string
  fingerprint_scope: string; real_model_status: string; real_client_status: string
  profiles: CompatibilityProfile[]; runs: { run: CompatibilityRun; stale: boolean }[]
}
function snapshot(value: CompatibilitySnapshot): CompatibilitySnapshot {
  if (value?.schema_version !== 1 || !Number.isSafeInteger(value.revision) || value.revision < 0
    || !Array.isArray(value.profiles) || !Array.isArray(value.runs)) throw new Error('不支持的兼容性数据格式')
  return value
}
export const compatibilityApi = {
  async list(signal?: AbortSignal) {
    return snapshot((await apiClient.get<CompatibilitySnapshot>('/api/admin/compatibility', { signal })).data)
  },
  async saveProfile(expected_revision: number, profile: { id: string; config: CompatibilityConfig }) {
    return snapshot((await apiClient.post<CompatibilitySnapshot>('/api/admin/compatibility/profiles', { expected_revision, profile })).data)
  },
  async run(expected_revision: number, profile: CompatibilityProfile) {
    return snapshot((await apiClient.post<CompatibilitySnapshot>('/api/admin/compatibility/runs', {
      expected_revision, profile_id: profile.id, expected_fingerprint: profile.config_fingerprint,
    })).data)
  },
}
export function compatibilityStatus(status: string): string {
  return ({ passed: '合成通过', failed: '失败', not_tested: '未测试', unsupported: '不支持' } as Record<string, string>)[status] ?? '未知（非通过）'
}
export function runSummary(cases: CompatibilityCase[]): string {
  if (cases.some(c => c.status === 'failed')) return 'failed'
  if (cases.some(c => !['passed', 'not_tested', 'unsupported'].includes(c.status))) return 'unknown'
  if (cases.some(c => c.status === 'unsupported')) return 'unsupported'
  const executed = cases.filter(c => !['real_client', 'real_model', 'cancel', 'compression'].includes(c.case))
  const names = ['basic_request', 'tool_definition', 'tool_return', 'multi_turn_tools', 'long_history', 'image', 'sync_response', 'tool_response', 'stream']
  return executed.length === 9 && names.every(name => executed.filter(c => c.case === name && c.status === 'passed').length === 1) ? 'passed' : 'not_tested'
}
