import { ARCA_NAME } from '@/constants/arcaBrand'

export type CcSwitchTargetApp = 'claude' | 'codex' | 'gemini' | 'grokbuild' | 'opencode' | 'openclaw' | 'hermes' | 'pi'

export interface CcSwitchTargetOption {
  value: CcSwitchTargetApp
  label: string
  icon: string
}

export interface BuildCcSwitchProviderImportUrlInput {
  targetApp: CcSwitchTargetApp
  baseUrl: string
  apiKey: string
  apiKeyName: string
  siteName?: string
  modelId?: string
  modelIds?: CcSwitchModelIds
  providerName?: string
  enabled?: boolean
}

export interface CcSwitchModelIds {
  default?: string
  haiku?: string
  sonnet?: string
  opus?: string
}

interface NormalizedCcSwitchModelIds {
  default: string
  haiku: string
  sonnet: string
  opus: string
}

export const CC_SWITCH_TARGET_OPTIONS: CcSwitchTargetOption[] = [
  { value: 'claude', label: 'Claude Code', icon: 'claude' },
  { value: 'codex', label: 'Codex CLI', icon: 'openai' },
  { value: 'gemini', label: 'Gemini CLI', icon: 'gemini' },
  { value: 'grokbuild', label: 'Grok Build', icon: 'grok' },
  { value: 'opencode', label: 'OpenCode', icon: 'opencode' },
  { value: 'openclaw', label: 'OpenClaw', icon: 'openclaw' },
  { value: 'hermes', label: 'Hermes', icon: 'hermes' },
  { value: 'pi', label: 'Pi', icon: 'pi' },
]

export const CC_SWITCH_COMPATIBILITY_VERSION = '4.0.13'

const TARGET_API_FORMAT = {
  claude: 'anthropic',
  codex: 'openai_responses',
  gemini: 'gemini_native',
  grokbuild: 'openai_responses',
  opencode: 'openai_chat',
  openclaw: 'openai_chat',
  hermes: 'openai_chat',
  pi: 'openai_chat',
} satisfies Record<CcSwitchTargetApp, string>

function buildUsageQueryScript(planName: string): string {
  return [
  '({',
  '  request: {',
  '    url: "{{baseUrl}}/api/ccswitch/usage",',
  '    method: "GET",',
  '    headers: {',
  '      "Authorization": "Bearer {{apiKey}}",',
  '      "Accept": "application/json",',
  '      "User-Agent": "cc-switch/1.0"',
  '    }',
  '  },',
  '  extractor: function(response) {',
  '    if (response && response.is_valid === false) {',
  '      return {',
  '        isValid: false,',
  '        invalidMessage: response.invalid_message || "查询失败"',
  '      };',
  '    }',
  '',
  '    return {',
  '      isValid: true,',
  `      planName: response.plan_name || ${JSON.stringify(planName)},`,
  '      remaining: response.remaining,',
  '      used: response.used,',
  '      total: response.total,',
  '      unit: response.unit || "USD",',
  '      extra: response.extra',
  '    };',
  '  }',
  '})',
].join('\n')
}

export function base64UrlEncodeUtf8(text: string): string {
  const bytes = new TextEncoder().encode(text)
  let binary = ''
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }

  return btoa(binary)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/g, '')
}

function normalizedBaseUrl(baseUrl: string): string {
  const url = new URL(baseUrl.trim())
  if (!['https:', 'http:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) {
    throw new Error('网关地址必须为不含凭据、查询参数和片段的 HTTP(S) 地址')
  }
  return url.toString().replace(/\/+$/g, '').replace(/\/v1$/, '')
}

function aetherV1BaseUrl(baseUrl: string): string {
  return `${normalizedBaseUrl(baseUrl)}/v1`
}

function ccSwitchEndpointForTarget(targetApp: CcSwitchTargetApp, baseUrl: string): string {
  if (targetApp === 'claude' || targetApp === 'gemini') {
    return normalizedBaseUrl(baseUrl)
  }
  return aetherV1BaseUrl(baseUrl)
}

function quoteTomlString(value: string): string {
  return JSON.stringify(value)
}

export function ccSwitchTargetLabel(targetApp: CcSwitchTargetApp): string {
  return CC_SWITCH_TARGET_OPTIONS.find(option => option.value === targetApp)?.label || targetApp
}

export function defaultCcSwitchProviderName(
  siteName?: string,
): string {
  return siteName?.trim() || ARCA_NAME
}

function buildCodexToml(baseUrl: string, modelId: string, providerName: string): string {
  return [
    'model_provider = "aether"',
    `model = ${quoteTomlString(modelId)}`,
    'model_reasoning_effort = "high"',
    'disable_response_storage = true',
    '',
    '[model_providers.aether]',
    `name = ${quoteTomlString(providerName)}`,
    `base_url = ${quoteTomlString(aetherV1BaseUrl(baseUrl))}`,
    'wire_api = "responses"',
    'requires_openai_auth = true',
    '',
  ].join('\n')
}

function normalizeCcSwitchModelIds(input: BuildCcSwitchProviderImportUrlInput): NormalizedCcSwitchModelIds {
  const defaultModel = input.modelIds?.default?.trim() || input.modelId?.trim() || ''
  const sonnet = input.modelIds?.sonnet?.trim() || defaultModel
  const haiku = input.modelIds?.haiku?.trim() || sonnet
  const opus = input.modelIds?.opus?.trim() || sonnet

  return {
    default: defaultModel,
    haiku,
    sonnet,
    opus,
  }
}

function buildCcSwitchConfig(
  input: BuildCcSwitchProviderImportUrlInput,
  providerName: string,
): Record<string, unknown> | undefined {
  const baseUrl = normalizedBaseUrl(input.baseUrl)
  const modelIds = normalizeCcSwitchModelIds(input)

  switch (input.targetApp) {
    case 'claude':
      return {
        env: {
          ANTHROPIC_AUTH_TOKEN: input.apiKey,
          ANTHROPIC_BASE_URL: baseUrl,
          ANTHROPIC_MODEL: modelIds.sonnet,
          ANTHROPIC_DEFAULT_HAIKU_MODEL: modelIds.haiku,
          ANTHROPIC_DEFAULT_SONNET_MODEL: modelIds.sonnet,
          ANTHROPIC_DEFAULT_OPUS_MODEL: modelIds.opus,
        },
      }
    case 'codex':
      return {
        auth: {
          OPENAI_API_KEY: input.apiKey,
        },
        config: buildCodexToml(baseUrl, modelIds.default, providerName),
      }
    case 'gemini':
      return {
        GEMINI_API_KEY: input.apiKey,
        GOOGLE_GEMINI_BASE_URL: baseUrl,
        GEMINI_MODEL: modelIds.default,
      }
    default:
      // v4.0.13 rebuilds additive-app settings from URL fields; extra config is discarded.
      return undefined
  }
}

export function buildCcSwitchProviderImportUrl(input: BuildCcSwitchProviderImportUrlInput): string {
  const providerName = input.providerName?.trim() || defaultCcSwitchProviderName(input.siteName)
  const icon = CC_SWITCH_TARGET_OPTIONS.find(option => option.value === input.targetApp)?.icon
  const modelIds = normalizeCcSwitchModelIds(input)
  const selectedModel = input.targetApp === 'claude' ? modelIds.sonnet : modelIds.default
  if (!input.apiKey.trim() || !selectedModel) throw new Error('请选择模型并提供有效的 API Key')
  if (['opencode', 'openclaw', 'hermes', 'pi'].includes(input.targetApp)) {
    const values = [providerName, input.baseUrl, input.apiKey, modelIds.default]
    if (values.some(value => /\$\{|\{env:|\{file:/.test(value))
      || (input.targetApp === 'pi' && (/^\s*!/.test(input.apiKey) || values.some(value => /\$[A-Za-z_]/.test(value))))) {
      throw new Error('CC Switch 导入不允许会展开为环境变量、文件或命令的配置值')
    }
  }
  const params = new URLSearchParams()

  params.set('resource', 'provider')
  params.set('app', input.targetApp)
  params.set('apiFormat', TARGET_API_FORMAT[input.targetApp])
  params.set('name', providerName)
  params.set('endpoint', ccSwitchEndpointForTarget(input.targetApp, input.baseUrl))
  params.set('apiKey', input.apiKey)
  params.set('model', selectedModel)
  if (input.targetApp === 'claude') {
    params.set('haikuModel', modelIds.haiku)
    params.set('sonnetModel', modelIds.sonnet)
    params.set('opusModel', modelIds.opus)
  }
  const config = buildCcSwitchConfig(input, providerName)
  if (config) {
    params.set('configFormat', 'json')
    params.set('config', base64UrlEncodeUtf8(JSON.stringify(config)))
  }
  params.set('enabled', input.enabled === false ? 'false' : 'true')
  // Official usage scripts are restricted to same-origin HTTPS requests.
  if (new URL(input.baseUrl).protocol === 'https:') {
    params.set('usageEnabled', 'true')
    params.set('usageBaseUrl', normalizedBaseUrl(input.baseUrl))
    params.set('usageAutoInterval', '30')
    params.set('usageScript', base64UrlEncodeUtf8(buildUsageQueryScript(providerName)))
  }
  if (icon) params.set('icon', icon)

  return `ccswitch://v1/import?${params.toString()}`
}
