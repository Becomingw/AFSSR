import { beforeEach, describe, expect, it, vi } from 'vitest'
import { meApi } from '../me'
import apiClient from '../client'
import { cache } from '@/utils/cache'
vi.mock('../client', () => ({ default: { get: vi.fn() } }))
beforeEach(() => { vi.clearAllMocks(); cache.clear() })
describe('personal Token history API', () => {
  it('uses the existing self-scoped endpoint and bypasses cached results on explicit refresh', async () => {
    const get = vi.mocked(apiClient.get)
    const response = (data: unknown) => ({ data, status: 200, statusText: 'OK', headers: {}, config: { headers: {} } } as Awaited<ReturnType<typeof apiClient.get>>)
    get.mockResolvedValueOnce(response({ days: [] })).mockResolvedValueOnce(response({ days: ['new'] }))
    expect(await meApi.getActivityHeatmap()).toEqual({ days: [] })
    expect(await meApi.getActivityHeatmap()).toEqual({ days: [] })
    expect(get).toHaveBeenCalledTimes(1)
    expect(await meApi.getActivityHeatmap(true)).toEqual({ days: ['new'] })
    expect(get).toHaveBeenLastCalledWith('/api/users/me/usage/heatmap')
    expect(get).toHaveBeenCalledTimes(2)
  })
})
