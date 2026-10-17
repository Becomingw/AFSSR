import { beforeEach, describe, expect, it, vi } from 'vitest'
const client = vi.hoisted(() => ({ get: vi.fn() }))
vi.mock('../client', () => ({ default: client }))
import { getRequestDiagnostic } from '../diagnostics'
beforeEach(() => { vi.clearAllMocks(); client.get.mockResolvedValue({ data: { state: 'unknown' } }) })
describe('diagnostic API', () => {
  it('uses separate owner/admin paths, encodes identifiers and only performs GET', async () => {
    const signal = new AbortController().signal
    await getRequestDiagnostic(false, 'req/1', signal)
    expect(client.get).toHaveBeenLastCalledWith('/api/requests/req%2F1/diagnostics', { signal })
    await getRequestDiagnostic(true, 'req-1')
    expect(client.get).toHaveBeenLastCalledWith('/api/admin/monitoring/diagnostics/req-1', { signal: undefined })
  })
})
