import { beforeEach, describe, expect, it, vi } from 'vitest'
import { accountApi } from '../account'
import apiClient from '../client'
vi.mock('../client', () => ({ default: { get: vi.fn() } }))
beforeEach(() => vi.clearAllMocks())
describe('self entitlements API', () => {
  it('accepts cancellation but no user or timezone overrides', async () => {
    vi.mocked(apiClient.get).mockResolvedValue({ data: { enabled: false }, status: 200, statusText: 'OK', headers: {}, config: { headers: {} } } as Awaited<ReturnType<typeof apiClient.get>>)
    const signal = new AbortController().signal
    expect(await accountApi.entitlements(signal)).toEqual({ enabled: false })
    expect(apiClient.get).toHaveBeenCalledWith('/api/users/me/entitlements', { signal })
  })
})
