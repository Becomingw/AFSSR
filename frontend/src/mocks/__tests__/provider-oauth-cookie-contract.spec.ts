import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/config/demo', () => ({
  isDemoMode: () => true,
  DEMO_ACCOUNTS: {
    admin: { email: 'admin@demo.arca.io', password: 'demo123' },
    user: { email: 'user@demo.arca.io', password: 'demo123' },
  },
}))

import { handleMockRequest, setMockUserToken } from '../handler'

describe('Claude Cookie authorization demo contracts', () => {
  beforeEach(() => {
    setMockUserToken('demo-access-token-admin')
  })

  it('keeps the single Cookie authorization response compatible', async () => {
    const response = await handleMockRequest({
      method: 'POST',
      url: '/api/admin/provider-oauth/providers/provider-claude/cookie-authorize',
      data: JSON.stringify({ cookie: 'sessionKey=single-secret' }),
    })

    expect(response?.data).toMatchObject({
      provider_type: 'claude_code',
      has_refresh_token: true,
    })
    expect(JSON.stringify(response?.data)).not.toContain('single-secret')
  })

  it('starts and reads a Cookie batch task without returning Cookie values', async () => {
    const startResponse = await handleMockRequest({
      method: 'POST',
      url: '/api/admin/provider-oauth/providers/provider-claude/cookie-authorize/tasks',
      data: JSON.stringify({
        cookies: ['sessionKey=success-secret', 'sessionKey=mock-fail-secret'],
      }),
    })
    const start = startResponse?.data as { task_id: string }

    expect(startResponse?.data).toMatchObject({
      import_kind: 'cookie_authorize',
      status: 'submitted',
      total: 2,
      processed: 0,
    })

    const statusResponse = await handleMockRequest({
      method: 'GET',
      url: `/api/admin/provider-oauth/providers/provider-claude/cookie-authorize/tasks/${start.task_id}`,
    })

    expect(statusResponse?.data).toMatchObject({
      task_id: start.task_id,
      provider_id: 'provider-claude',
      provider_type: 'claude_code',
      import_kind: 'cookie_authorize',
      status: 'completed',
      total: 2,
      success: 1,
      failed: 1,
      error_samples: [{ index: 1, status: 'error' }],
    })
    expect(JSON.stringify(statusResponse?.data)).not.toContain('success-secret')
    expect(JSON.stringify(statusResponse?.data)).not.toContain('mock-fail-secret')
  })
})
