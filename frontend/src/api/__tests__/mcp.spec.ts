import { beforeEach, describe, expect, it, vi } from 'vitest'
const client = vi.hoisted(() => ({ get: vi.fn(), post: vi.fn(), delete: vi.fn(), patch: vi.fn() }))
vi.mock('../client', () => ({ default: client }))
import { listMcpKeys, createMcpKey, revokeMcpKey, updateMcpKey } from '../mcp'

beforeEach(() => vi.clearAllMocks())
describe('MCP self-service API', () => {
  it('lists the current user keys without a user-supplied owner', async () => {
    client.get.mockResolvedValue({ data: { items: [] } })
    expect(await listMcpKeys()).toEqual({ items: [] })
    expect(client.get).toHaveBeenCalledWith('/api/mcp/keys')
  })
  it('marks creation and revocation as interactive and encodes identifiers', async () => {
    client.post.mockResolvedValue({ data: { token: 'fixture-only' } })
    client.delete.mockResolvedValue({})
    const input = { name: 'Agent', read_only: true, expires_in_days: 30 }
    expect(await createMcpKey(input)).toEqual({ token: 'fixture-only' })
    expect(client.post).toHaveBeenCalledWith('/api/mcp/keys', input, { headers: { 'X-Arca-MCP-Key-Management': '1' } })
    await revokeMcpKey('id/1')
    expect(client.delete).toHaveBeenCalledWith('/api/mcp/keys/id%2F1', { headers: { 'X-Arca-MCP-Key-Management': '1' } })
  })
})

it('sends explicit null and interactive headers when changing expiry', async () => {
  client.patch.mockResolvedValue({ data: { data: { id: 'id/1' } } })
  expect(await updateMcpKey('id/1', { expires_in_days: null })).toEqual({ data: { id: 'id/1' } })
  expect(client.patch).toHaveBeenCalledWith('/api/mcp/keys/id%2F1', { expires_in_days: null }, { headers: { 'X-Arca-MCP-Key-Management': '1' } })
})
