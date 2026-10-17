import { describe, expect, it } from 'vitest'

import {
  base64UrlEncodeUtf8,
  buildCcSwitchProviderImportUrl,
  type CcSwitchTargetApp,
} from '@/features/api-keys/utils/ccswitchImport'

function decodeBase64UrlJson(value: string): unknown {
  const padded = `${value}${'='.repeat((4 - (value.length % 4)) % 4)}`
    .replace(/-/g, '+')
    .replace(/_/g, '/')
  const binary = atob(padded)
  const bytes = Uint8Array.from(binary, char => char.charCodeAt(0))
  return JSON.parse(new TextDecoder().decode(bytes))
}

function parseImportUrl(url: string) {
  const parsed = new URL(url)
  const params = parsed.searchParams
  const config = params.get('config')
  return {
    parsed,
    params,
    config: config ? decodeBase64UrlJson(config) as Record<string, unknown> : {},
  }
}

const baseInput = {
  baseUrl: 'https://aether.example.com',
  apiKey: 'sk-user-live-1',
  apiKeyName: 'primary',
  siteName: 'Aether Local',
  modelId: 'gpt-5',
}

describe('ccswitchImport', () => {
  it('encodes UTF-8 text as base64url without padding', () => {
    const encoded = base64UrlEncodeUtf8('Aether 中文+/=')

    expect(encoded).not.toContain('+')
    expect(encoded).not.toContain('/')
    expect(encoded).not.toContain('=')
    expect(new TextDecoder().decode(Uint8Array.from(atob(`${encoded}${'='.repeat((4 - (encoded.length % 4)) % 4)}`.replace(/-/g, '+').replace(/_/g, '/')), char => char.charCodeAt(0)))).toBe('Aether 中文+/=')
  })

  it('builds a Claude Code import URL with separate Claude model env fields', () => {
    const { params, config } = parseImportUrl(buildCcSwitchProviderImportUrl({
      ...baseInput,
      targetApp: 'claude',
      modelIds: {
        haiku: 'gpt-5.4-mini',
        sonnet: 'gpt-5',
        opus: 'gpt-5.1-pro',
      },
    }))

    expect(params.get('resource')).toBe('provider')
    expect(params.get('app')).toBe('claude')
    expect(params.get('name')).toBe('Aether Local')
    expect(params.get('icon')).toBe('claude')
    expect(params.get('enabled')).toBe('true')
    expect(params.get('configFormat')).toBe('json')
    expect(params.get('usageEnabled')).toBe('true')
    expect(params.get('usageBaseUrl')).toBe('https://aether.example.com')
    expect(params.has('usageApiKey')).toBe(false)
    expect(params.get('apiFormat')).toBe('anthropic')
    expect(params.get('usageAutoInterval')).toBe('30')
    expect(params.get('endpoint')).toBe('https://aether.example.com')
    expect(params.get('apiKey')).toBe('sk-user-live-1')
    expect(params.get('model')).toBe('gpt-5')
    expect(params.get('haikuModel')).toBe('gpt-5.4-mini')
    expect(params.get('sonnetModel')).toBe('gpt-5')
    expect(params.get('opusModel')).toBe('gpt-5.1-pro')
    const usageScript = params.get('usageScript') || ''
    expect(usageScript).not.toBe('')
    expect(usageScript).not.toContain('(')
    expect(usageScript).not.toContain('+')
    expect(usageScript).not.toContain('/')
    const paddedUsageScript = `${usageScript}${'='.repeat((4 - (usageScript.length % 4)) % 4)}`
      .replace(/-/g, '+')
      .replace(/_/g, '/')
    const decodedUsageScript = new TextDecoder().decode(
      Uint8Array.from(atob(paddedUsageScript), char => char.charCodeAt(0)),
    )
    expect(decodedUsageScript).toContain('/api/ccswitch/usage')
    expect(decodedUsageScript).toContain('Authorization')
    expect(config).toEqual({
      env: {
        ANTHROPIC_AUTH_TOKEN: 'sk-user-live-1',
        ANTHROPIC_BASE_URL: 'https://aether.example.com',
        ANTHROPIC_MODEL: 'gpt-5',
        ANTHROPIC_DEFAULT_HAIKU_MODEL: 'gpt-5.4-mini',
        ANTHROPIC_DEFAULT_SONNET_MODEL: 'gpt-5',
        ANTHROPIC_DEFAULT_OPUS_MODEL: 'gpt-5.1-pro',
      },
    })
  })

  it('builds a Codex import URL with auth and TOML config', () => {
    const { params, config } = parseImportUrl(buildCcSwitchProviderImportUrl({
      ...baseInput,
      targetApp: 'codex',
    }))

    expect(params.get('app')).toBe('codex')
    expect(params.get('apiFormat')).toBe('openai_responses')
    expect(params.get('icon')).toBe('openai')
    expect(params.get('endpoint')).toBe('https://aether.example.com/v1')
    expect(params.get('apiKey')).toBe('sk-user-live-1')
    expect(params.get('model')).toBe('gpt-5')
    expect(config.auth).toEqual({ OPENAI_API_KEY: 'sk-user-live-1' })
    expect(config.config).toContain('model_provider = "aether"')
    expect(config.config).toContain('model = "gpt-5"')
    expect(config.config).toContain('base_url = "https://aether.example.com/v1"')
    expect(config.config).toContain('wire_api = "responses"')
  })

  it('accepts explicit Claude slots without a separate default model', () => {
    const { params } = parseImportUrl(buildCcSwitchProviderImportUrl({
      ...baseInput, targetApp: 'claude', modelId: undefined, modelIds: { sonnet: 'public-sonnet' },
    }))
    expect(params.get('model')).toBe('public-sonnet')
  })

  it.each([
    ['gemini', {
      GEMINI_API_KEY: 'sk-user-live-1',
      GOOGLE_GEMINI_BASE_URL: 'https://aether.example.com',
      GEMINI_MODEL: 'gpt-5',
    }],
    ['grokbuild', {}],
    ['opencode', {}],
    ['openclaw', {}],
    ['hermes', {}],
    ['pi', {}],
  ] satisfies Array<[CcSwitchTargetApp, Record<string, unknown>]>)('builds %s import config', (targetApp, expectedConfig) => {
    const { params, config } = parseImportUrl(buildCcSwitchProviderImportUrl({
      ...baseInput,
      targetApp,
    }))

    expect(params.get('app')).toBe(targetApp)
    expect(params.get('endpoint')).toBe(
      targetApp === 'gemini'
        ? 'https://aether.example.com'
        : 'https://aether.example.com/v1',
    )
    expect(params.get('apiKey')).toBe('sk-user-live-1')
    expect(params.get('model')).toBe('gpt-5')
    expect(params.get('apiFormat')).toBe(targetApp === 'gemini' ? 'gemini_native' : targetApp === 'grokbuild' ? 'openai_responses' : 'openai_chat')
    expect(config).toEqual(expectedConfig)
    expect(params.has('configUrl')).toBe(false)
    if (targetApp !== 'gemini') expect(params.has('config')).toBe(false)
  })

  it('normalizes an existing /v1 suffix and preserves gateway path prefixes', () => {
    const { params } = parseImportUrl(buildCcSwitchProviderImportUrl({
      ...baseInput, targetApp: 'opencode', baseUrl: 'https://example.com/gateway/v1/',
    }))
    expect(params.get('endpoint')).toBe('https://example.com/gateway/v1')
    expect(params.get('usageBaseUrl')).toBe('https://example.com/gateway')
  })

  it('does not enable an unsupported HTTP usage query', () => {
    const { params } = parseImportUrl(buildCcSwitchProviderImportUrl({
      ...baseInput, targetApp: 'opencode', baseUrl: 'http://127.0.0.1:8084', enabled: false,
    }))
    expect(params.has('usageScript')).toBe(false)
    expect(params.has('usageEnabled')).toBe(false)
    expect(params.get('enabled')).toBe('false')
  })

  it('round trips literal credentials, unicode names and reserved URL characters', () => {
    const { params } = parseImportUrl(buildCcSwitchProviderImportUrl({
      ...baseInput, targetApp: 'opencode', providerName: '网关 +/&', apiKey: 'sk-+/=&?%#', modelId: 'team/model+v1',
    }))
    expect(params.get('name')).toBe('网关 +/&')
    expect(params.get('apiKey')).toBe('sk-+/=&?%#')
    expect(params.get('model')).toBe('team/model+v1')
  })

  it.each(['{env:SECRET}', '{file:/etc/passwd}', '${SECRET}'])('rejects dynamic references: %s', (modelId) => {
    expect(() => buildCcSwitchProviderImportUrl({ ...baseInput, targetApp: 'opencode', modelId })).toThrow('不允许')
  })

  it('rejects Pi command and environment credentials', () => {
    for (const apiKey of ['!cat /etc/passwd', '$SECRET']) {
      expect(() => buildCcSwitchProviderImportUrl({ ...baseInput, targetApp: 'pi', apiKey })).toThrow('不允许')
    }
  })

  it('rejects credential-bearing or non-HTTP gateway URLs', () => {
    for (const baseUrl of ['file:///tmp/key', 'https://user:pass@example.com', 'https://example.com?token=secret']) {
      expect(() => buildCcSwitchProviderImportUrl({ ...baseInput, targetApp: 'codex', baseUrl })).toThrow()
    }
  })
})
