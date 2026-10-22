import apiClient from './client'

export interface McpKey {
  id: string
  name: string
  token_prefix?: string
  is_active: boolean
  expires_at_unix_secs?: number | null
  last_used_at_unix_secs?: number
  permissions?: string[]
  allowed_ips?: unknown
}
export interface McpKeysResponse {
  items: McpKey[]
  user_id: string
  role: string
  read_permissions: string[]
  write_permissions: string[]
  endpoint: string
  enabled: boolean
  max_keys: number
}
export interface CreateMcpKey {
  name: string
  read_only: boolean
  expires_in_days?: number | null
  allowed_ips?: string[]
  permissions?: string[]
}
const headers = { 'X-Arca-MCP-Key-Management': '1' }
export async function listMcpKeys(): Promise<McpKeysResponse> {
  return (await apiClient.get<McpKeysResponse>('/api/mcp/keys')).data
}
export async function createMcpKey(input: CreateMcpKey): Promise<{ token: string; data: McpKey }> {
  return (await apiClient.post<{ token: string; data: McpKey }>('/api/mcp/keys', input, { headers })).data
}
export async function revokeMcpKey(id: string): Promise<void> {
  await apiClient.delete(`/api/mcp/keys/${encodeURIComponent(id)}`, { headers })
}

export async function updateMcpKey(id: string, input: { expires_in_days: number | null }): Promise<{ data: McpKey }> {
  return (await apiClient.patch<{ data: McpKey }>(`/api/mcp/keys/${encodeURIComponent(id)}`, input, { headers })).data
}
