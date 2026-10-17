import { beforeEach, describe, expect, it, vi } from 'vitest'
const client = vi.hoisted(() => ({ get: vi.fn() }))
vi.mock('../client', () => ({ default: client }))
import { getLeaderboard } from '../leaderboard'
beforeEach(() => { vi.clearAllMocks(); client.get.mockResolvedValue({ data: {} }) })
describe('leaderboard API scopes', () => {
  it('uses the masked route for ordinary users, without requesting names or cost overrides', async () => {
    await getLeaderboard(false, 'this_week', 480)
    expect(client.get).toHaveBeenCalledWith('/api/leaderboard', { params: { preset: 'this_week', tz_offset_minutes: 480 }, signal: undefined })
  })
  it('uses the protected stats route for administrators and supports cancelling stale requests', async () => {
    const controller = new AbortController()
    await getLeaderboard(true, 'last_week', 0, controller.signal)
    expect(client.get).toHaveBeenCalledWith('/api/admin/stats/leaderboard/overview', { params: { preset: 'last_week', tz_offset_minutes: 0 }, signal: controller.signal })
  })
})
