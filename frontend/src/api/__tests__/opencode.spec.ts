import { afterEach, describe, expect, it, vi } from 'vitest'
import { downloadOpenCodeConfig, getOpenCodeConfig } from '../opencode'

const config = {
  provider: {
    aether: {
      npm: '@ai-sdk/openai-compatible',
      options: { baseURL: 'https://gateway.example/v1' },
      models: {
        'public-alias': { name: 'Public', limit: { context: 64000, output: 8000 }, tool_call: true, reasoning: false },
        'other-model': { name: 'Other', cost: { input: 1, output: 2 } },
      },
    },
  },
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
  vi.restoreAllMocks()
  vi.useRealTimers()
})

describe('OpenCode client configuration', () => {
  it('authenticates with the selected API key without cookies, redirects or browser JWT', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => config })
    vi.stubGlobal('fetch', fetchMock)
    expect(await getOpenCodeConfig('sk-selected')).toEqual(config)
    expect(fetchMock).toHaveBeenCalledWith('/api/opencode/config', expect.objectContaining({
      headers: { Authorization: 'Bearer sk-selected', Accept: 'application/json' },
      credentials: 'omit', cache: 'no-store', redirect: 'error',
    }))
  })

  it('does not expose a potentially sensitive error response body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 403, json: async () => ({ key: 'secret' }) }))
    await expect(getOpenCodeConfig('sk-test')).rejects.toThrow('HTTP 403')
  })

  it('honors the configured frontend API base without using a provider upstream URL', async () => {
    vi.stubEnv('VITE_API_URL', 'https://gateway.example/control/')
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => config })
    vi.stubGlobal('fetch', fetchMock)
    await getOpenCodeConfig('sk-selected')
    expect(fetchMock).toHaveBeenCalledWith('https://gateway.example/control/api/opencode/config', expect.objectContaining({
      credentials: 'omit', redirect: 'error',
    }))
  })

  it('rejects a malformed catalog', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ provider: {} }) }))
    await expect(getOpenCodeConfig('sk-test')).rejects.toThrow('格式不正确')
  })

  it('exports every authorized model and metadata without mutating the credential-free catalog', () => {
    vi.useFakeTimers()
    let contents = ''
    vi.stubGlobal('Blob', class extends Blob {
      constructor(parts: BlobPart[], options: BlobPropertyBag) {
        super(parts, options)
        contents = parts.join('')
      }
    })
    vi.stubGlobal('URL', class extends URL {
      static createObjectURL = vi.fn(() => 'blob:config')
      static revokeObjectURL = vi.fn()
    })
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
    downloadOpenCodeConfig(config, 'sk-selected', 'public-alias')
    const exported = JSON.parse(contents)
    expect(exported.model).toBe('aether/public-alias')
    expect(exported.provider.aether.models).toEqual(config.provider.aether.models)
    expect(exported.provider.aether.options.apiKey).toBe('sk-selected')
    expect(config.provider.aether.options).not.toHaveProperty('apiKey')
    expect(click).toHaveBeenCalledOnce()
    vi.runAllTimers()
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:config')
  })

  it('refuses an unauthorized default or empty catalog', () => {
    expect(() => downloadOpenCodeConfig(config, 'sk-test', 'not-allowed')).toThrow('无权')
    expect(() => downloadOpenCodeConfig({ provider: { aether: { ...config.provider.aether, models: {} } } }, 'sk-test')).toThrow('没有可用')
  })
})
