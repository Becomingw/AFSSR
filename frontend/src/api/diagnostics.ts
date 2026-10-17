import apiClient from './client'

export interface RequestDiagnostic {
  schema_version: number
  request_id: string
  state: 'unknown' | 'in_progress' | 'failed' | 'known'
  final_outcome: 'unknown' | 'in_progress' | 'completed' | 'failed' | 'cancelled'
  has_attempt_failures: boolean
  stages: { id: string; status: 'known' | 'failed' | 'unknown'; code: string; evidence: string[] }[]
  summary: string
}

export async function getRequestDiagnostic(admin: boolean, requestId: string, signal?: AbortSignal): Promise<RequestDiagnostic> {
  const path = admin ? `/api/admin/monitoring/diagnostics/${encodeURIComponent(requestId)}` : `/api/requests/${encodeURIComponent(requestId)}/diagnostics`
  return (await apiClient.get<RequestDiagnostic>(path, { signal })).data
}
