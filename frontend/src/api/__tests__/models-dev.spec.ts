import { beforeEach, describe, expect, it, vi } from 'vitest'

const apiMocks = vi.hoisted(() => ({
  get: vi.fn(),
  put: vi.fn(),
  delete: vi.fn(),
  post: vi.fn(),
}))

vi.mock('@/api/client', () => ({
  default: apiMocks,
}))

import {
  clearModelsDevCache,
  getExternalModelsAccessConfig,
  getModelsDevList,
  refreshModelsDevList,
  updateExternalModelsAccessConfig,
} from '@/api/models-dev'

beforeEach(() => {
  clearModelsDevCache()
  localStorage.clear()
  apiMocks.get.mockReset()
  apiMocks.put.mockReset()
  apiMocks.delete.mockReset()
  apiMocks.post.mockReset()
  apiMocks.delete.mockResolvedValue({ data: { cleared: true } })
})

describe('external models access config', () => {
  it('reads the configured proxy node', async () => {
    apiMocks.get.mockResolvedValue({ data: { proxy_node_id: 'proxy-1' } })

    await expect(getExternalModelsAccessConfig()).resolves.toEqual({
      proxy_node_id: 'proxy-1',
    })
    expect(apiMocks.get).toHaveBeenCalledWith('/api/admin/models/external/config')
  })

  it.each([
    ['a proxy node', 'proxy-1'],
    ['direct access', null],
  ] as const)('updates %s without invalidating the snapshot', async (_label, proxyNodeId) => {
    localStorage.setItem('models_dev_cache', JSON.stringify({ timestamp: Date.now(), data: {} }))
    apiMocks.put.mockResolvedValue({
      data: { proxy_node_id: proxyNodeId, cache_cleared: true },
    })

    await expect(updateExternalModelsAccessConfig(proxyNodeId)).resolves.toEqual({
      proxy_node_id: proxyNodeId,
      cache_cleared: true,
    })
    expect(apiMocks.put).toHaveBeenCalledWith(
      '/api/admin/models/external/config',
      { proxy_node_id: proxyNodeId },
    )
    expect(localStorage.getItem('models_dev_cache')).not.toBeNull()
  })
})

describe('getModelsDevList', () => {
  it('uses current modalities and experimental mode pricing while keeping legacy fallbacks', async () => {
    apiMocks.get.mockResolvedValue({
      data: {
        openai: {
          id: 'openai',
          name: 'OpenAI',
          official: true,
          models: {
            'gpt-test': {
              id: 'gpt-test',
              name: 'GPT Test',
              input: ['text'],
              output: ['text'],
              modalities: {
                input: ['text', 'image'],
                output: ['text', 'image'],
              },
              cost: {
                input: 2,
                output: 4,
                tiers: [{
                  input: 4,
                  output: 8,
                  tier: { type: 'context', size: 100_000 },
                }],
              },
              experimental: {
                modes: {
                  fast: {
                    cost: { input: 4, output: 8 },
                    provider: { body: { service_tier: 'priority' } },
                  },
                },
              },
            },
            legacy: {
              id: 'legacy',
              name: 'Legacy',
              input: ['text', 'image'],
              output: ['text'],
              cost: { input: 1, output: 2 },
            },
            'audio-priced': {
              id: 'audio-priced',
              name: 'Audio Priced',
              cost: { input: 1, output: 2, input_audio: 4 },
            },
          },
        },
      },
    })

    const models = await getModelsDevList()
    const current = models.find(model => model.modelId === 'gpt-test')
    const legacy = models.find(model => model.modelId === 'legacy')
    const audioPriced = models.find(model => model.modelId === 'audio-priced')

    expect(current).toMatchObject({
      supportsVision: true,
      inputModalities: ['text', 'image'],
      outputModalities: ['text', 'image'],
      tieredPricing: {
        processing_tiers: { priority: { price_multiplier: 2 } },
      },
    })
    expect(legacy).toMatchObject({
      supportsVision: true,
      inputModalities: ['text', 'image'],
      outputModalities: ['text'],
    })
    expect(audioPriced).toMatchObject({
      inputPrice: 1,
      outputPrice: 2,
      pricingUnsupportedFields: ['input_audio'],
    })
    expect(audioPriced?.tieredPricing).toBeUndefined()
  })

  it('explicitly refreshes and synchronizes without a delete-before-fetch gap', async () => {
    apiMocks.get.mockResolvedValue({
      data: {
        openai: {
          name: 'OpenAI',
          official: true,
          models: {
            'gpt-test': {
              id: 'gpt-test',
              name: 'GPT Test',
              cost: { input: 2, output: 4 },
            },
          },
        },
      },
    })

    apiMocks.post.mockImplementation(async () => ({ data: { catalog: (await apiMocks.get()).data,
      sync: { updated: 1, unchanged: 0, unmatched: 0, failed: [] } } }))
    await getModelsDevList(false)
    await refreshModelsDevList(false)

    expect(apiMocks.delete).not.toHaveBeenCalled()
    expect(apiMocks.post).toHaveBeenCalledWith('/api/admin/models/external/refresh', {}, { timeout: 120_000 })
    expect(apiMocks.get).toHaveBeenCalledTimes(2)
  })

  it('deduplicates concurrent loads and retains snapshots beyond the old TTL', async () => {
    const catalog = { openai: { name: 'OpenAI', official: true, models: {} } }
    let finish!: (value: { data: typeof catalog }) => void
    apiMocks.get.mockReturnValue(new Promise(resolve => { finish = resolve }))
    const first = getModelsDevList(false), second = getModelsDevList(false)
    expect(apiMocks.get).toHaveBeenCalledTimes(1)
    finish({ data: catalog })
    await first; await second
    vi.spyOn(Date, 'now').mockReturnValue(Date.now() + 86400_000)
    await getModelsDevList(false)
    expect(apiMocks.get).toHaveBeenCalledTimes(1)
    vi.restoreAllMocks()
  })

  it('preserves the last good snapshot when explicit refresh fails', async () => {
    apiMocks.get.mockResolvedValue({ data: { openai: { name: 'OpenAI', official: true, models: { model: { id: 'model', name: 'Model' } } } } })
    const before = await getModelsDevList(false)
    apiMocks.post.mockRejectedValue(new Error('offline'))
    await expect(refreshModelsDevList(false)).rejects.toThrow('offline')
    expect(await getModelsDevList(false)).toEqual(before)
    expect(apiMocks.get).toHaveBeenCalledTimes(1)
  })
})
