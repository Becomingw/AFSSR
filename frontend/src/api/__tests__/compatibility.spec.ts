import { beforeEach, describe, expect, it, vi } from 'vitest'
import apiClient from '../client'
import { compatibilityApi, compatibilityStatus, runSummary, type CompatibilityProfile } from '../compatibility'
vi.mock('../client', () => ({ default: { get: vi.fn(), post: vi.fn() } }))
const value = { schema_version: 1, revision: 7, profiles: [], runs: [] }
const response = (data: unknown) => ({ data, status: 200, statusText: 'OK', headers: {}, config: { headers: {} } }) as Awaited<ReturnType<typeof apiClient.get>>
beforeEach(() => vi.clearAllMocks())
describe('offline compatibility API', () => {
  it('uses exact paths and CAS without credentials or caller-supplied results', async () => {
    vi.mocked(apiClient.get).mockResolvedValue(response(value))
    vi.mocked(apiClient.post).mockResolvedValue(response(value))
    const signal = new AbortController().signal
    await compatibilityApi.list(signal)
    expect(apiClient.get).toHaveBeenCalledWith('/api/admin/compatibility', { signal })
    const profile: CompatibilityProfile = { id: 'fixture-id', config_fingerprint: 'fingerprint', config: { client: 'codex', client_version: 'synthetic-1', model: 'fixture', mapped_model: 'fixture-upstream', client_protocol: 'openai:responses', upstream_format: 'claude:messages' } }
    await compatibilityApi.saveProfile(7, { id: profile.id, config: profile.config })
    expect(apiClient.post).toHaveBeenLastCalledWith('/api/admin/compatibility/profiles', { expected_revision: 7, profile: { id: profile.id, config: profile.config } })
    await compatibilityApi.run(7, profile)
    expect(apiClient.post).toHaveBeenLastCalledWith('/api/admin/compatibility/runs', { expected_revision: 7, profile_id: profile.id, expected_fingerprint: 'fingerprint' })
  })
  it('rejects unknown schema and never promotes unknown status to passed', async () => {
    vi.mocked(apiClient.get).mockResolvedValue(response({ ...value, schema_version: 2 }))
    await expect(compatibilityApi.list()).rejects.toThrow('不支持')
    expect(compatibilityStatus('future_success')).toBe('未知（非通过）')
    expect(runSummary([])).toBe('not_tested')
    expect(runSummary([{ case: 'basic_request', status: 'unknown', reason: '' }])).toBe('unknown')
    expect(runSummary([{ case: 'basic_request', status: 'unsupported', reason: '' }])).toBe('unsupported')
  })
})
