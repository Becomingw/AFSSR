import { beforeEach, describe, expect, it, vi } from 'vitest'
const client = vi.hoisted(() => ({ get: vi.fn(), post: vi.fn() }))
vi.mock('../client', () => ({ default: client }))
import { listFeedback, getFeedback, createFeedback, commandFeedback } from '../feedback'
beforeEach(() => { vi.clearAllMocks(); client.get.mockResolvedValue({ data: {} }); client.post.mockResolvedValue({ data: {} }) })
describe('feedback API boundaries', () => {
  it('separates own-user and administrative list routes and forwards poll cursors', async () => {
    await listFeedback(false, { after: 4, limit: 20 })
    expect(client.get).toHaveBeenLastCalledWith('/api/feedback', { params: { after: 4, limit: 20 } })
    await listFeedback(true)
    expect(client.get).toHaveBeenLastCalledWith('/api/admin/feedback', { params: {} })
    await getFeedback(false, 'id/1', 7)
    expect(client.get).toHaveBeenLastCalledWith('/api/feedback/id%2F1', { params: { after_event: 7, limit: 20 } })
  })
  it('never accepts actor or owner overrides and retains CAS and idempotency fields', async () => {
    const command = { action: 'reply' as const, content: 'Test', expected_version: 3, client_request_id: 'request-id' }
    await commandFeedback(true, 'issue-id', command)
    expect(client.post).toHaveBeenCalledWith('/api/admin/feedback/issue-id/commands', command)
    const input = { title: 'Test', description: 'Test', category: 'bug' as const, client_request_id: 'request-id' }
    await createFeedback(input)
    expect(client.post).toHaveBeenLastCalledWith('/api/feedback', input)
    await createFeedback(input, true)
    expect(client.post).toHaveBeenLastCalledWith('/api/admin/feedback', input)
  })
})
