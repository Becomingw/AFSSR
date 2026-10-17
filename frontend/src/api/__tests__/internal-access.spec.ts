import { beforeEach, describe, expect, it, vi } from 'vitest'
import { loadInternalAccess, saveInternalAccess } from '../internal-access'

const mocks = vi.hoisted(() => ({ get: vi.fn(), put: vi.fn() }))
vi.mock('../admin', () => ({ adminApi: { getSystemConfig: mocks.get } }))
vi.mock('../client', () => ({ default: { put: mocks.put } }))

beforeEach(() => vi.clearAllMocks())

describe('internal money policy API', () => {
  it('allows first-time configuration after a missing policy and uses a null CAS snapshot', async () => {
    mocks.get.mockResolvedValueOnce({ value: { timezone: 'UTC', groups: {}, users: {} } })
    await loadInternalAccess()
    mocks.get.mockRejectedValueOnce({ response: { status: 404 } })
    const draft = await loadInternalAccess()
    expect(draft).toEqual({ timezone: 'Asia/Shanghai', groups: {}, users: {} })
    mocks.put.mockResolvedValueOnce({})
    await saveInternalAccess(draft)
    expect(mocks.put).toHaveBeenLastCalledWith(expect.any(String), expect.objectContaining({ value: draft, expected_value: null }))
  })
  it.each([401, 403, 500, 503])('does not treat HTTP %s as a missing policy', async status => {
    const error = { response: { status } }
    mocks.get.mockRejectedValueOnce(error)
    await expect(loadInternalAccess()).rejects.toBe(error)
    expect(mocks.put).not.toHaveBeenCalled()
  })
  it('does not treat a network failure as an empty policy', async () => {
    const error = new Error('network unavailable')
    mocks.get.mockRejectedValueOnce(error)
    await expect(loadInternalAccess()).rejects.toBe(error)
    expect(mocks.put).not.toHaveBeenCalled()
  })
  it('retains fractional model and total money and sends an immutable expected snapshot', async () => {
    const value = { timezone: 'Asia/Shanghai', groups: {}, users: {}, default: {
      unlimited: false, five_hour_budget_usd: 1.25, weekly_budget_usd: 5.5, monthly_budget_usd: 20,
      models: { glm: { daily_budget_usd: 0.15 } },
    } }
    mocks.get.mockResolvedValue({ value })
    mocks.put.mockResolvedValue({})
    const draft = await loadInternalAccess()
    draft.default!.five_hour_budget_usd = 2.5
    await saveInternalAccess(draft)
    expect(mocks.put).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({
      value: draft, expected_value: expect.objectContaining({ default: expect.objectContaining({ five_hour_budget_usd: 1.25 }) }),
    }))
  })
  it.each(['default', 'groups', 'users'])('refuses to silently reinterpret legacy %s request caps as dollars', async target => {
    const old = { models: { glm: { daily_requests: 30 } } }
    mocks.get.mockResolvedValue({ value: {
      timezone: 'Asia/Shanghai', groups: target === 'groups' ? { research: old } : {},
      users: target === 'users' ? { user: old } : {}, default: target === 'default' ? old : null,
    } })
    await expect(loadInternalAccess()).rejects.toThrow('旧次数额度不能自动换算为金额')
    expect(mocks.put).not.toHaveBeenCalled()
  })
  it('saves definitions and family budgets atomically and preserves the CAS snapshot across failed writes', async () => {
    const original = {
      timezone: 'Asia/Shanghai', groups: {}, users: {},
      model_families: { 'f-one': { name: 'Original', models: ['public-a', 'public-b'] } },
      default: { unlimited: false, models: {}, families: { 'f-one': { daily_budget_usd: 0.3 } } },
    }
    mocks.get.mockResolvedValue({ value: original })
    const draft = await loadInternalAccess()
    const snapshot = JSON.parse(JSON.stringify(draft))
    draft.model_families!['f-one']!.name = 'Renamed'
    draft.model_families!['f-one']!.models = ['public-b']
    draft.default!.families!['f-one']!.daily_budget_usd = 1.5
    mocks.put.mockRejectedValueOnce(new Error('conflict')).mockResolvedValue({})
    await expect(saveInternalAccess(draft)).rejects.toThrow('conflict')
    await saveInternalAccess(draft)
    expect(mocks.put.mock.calls[0]![1]).toMatchObject({ value: draft, expected_value: snapshot })
    expect(mocks.put.mock.calls[1]![1]).toMatchObject({ value: draft, expected_value: snapshot })
    const savedSnapshot = JSON.parse(JSON.stringify(draft))
    draft.model_families!['f-one']!.name = 'Third'
    await saveInternalAccess(draft)
    expect(mocks.put.mock.calls[2]![1].expected_value).toEqual(savedSnapshot)
  })
})
