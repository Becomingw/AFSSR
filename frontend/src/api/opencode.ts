export interface OpenCodeModelConfig {
  name?: string
  limit?: { context?: number; output?: number }
  cost?: Record<string, number>
  tool_call?: boolean
  reasoning?: boolean
  attachment?: boolean
  temperature?: boolean
  modalities?: { input?: string[]; output?: string[] }
}

export interface OpenCodeClientConfig {
  $schema?: string
  provider: {
    aether: {
      npm: string
      name?: string
      options: { baseURL: string; apiKey?: string }
      models: Record<string, OpenCodeModelConfig>
    }
  }
}

export async function getOpenCodeConfig(apiKey: string): Promise<OpenCodeClientConfig> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), 15_000)
  try {
    // Session interceptors must not replace the selected API key with a browser JWT.
    const apiBase = (import.meta.env.VITE_API_URL || '').replace(/\/+$/g, '')
    const response = await fetch(`${apiBase}/api/opencode/config`, {
      headers: { Authorization: `Bearer ${apiKey}`, Accept: 'application/json' },
      credentials: 'omit',
      cache: 'no-store',
      redirect: 'error',
      signal: controller.signal,
    })
    if (!response.ok) throw new Error(`OpenCode 配置获取失败（HTTP ${response.status}）`)
    const config = await response.json() as OpenCodeClientConfig
    const provider = config?.provider?.aether
    if (provider?.npm !== '@ai-sdk/openai-compatible'
      || typeof provider.options?.baseURL !== 'string'
      || !provider.models || typeof provider.models !== 'object' || Array.isArray(provider.models)) {
      throw new Error('OpenCode 模型目录格式不正确')
    }
    return config
  } finally {
    clearTimeout(timer)
  }
}

export function downloadOpenCodeConfig(config: OpenCodeClientConfig, apiKey: string, modelId?: string): void {
  const provider = config.provider.aether
  if (Object.keys(provider.models).length === 0) throw new Error('当前 API Key 没有可用的 OpenCode 模型')
  if (modelId && !Object.prototype.hasOwnProperty.call(provider.models, modelId)) throw new Error('当前 API Key 无权使用所选模型')
  const contents = {
    $schema: 'https://opencode.ai/config.json',
    ...(modelId ? { model: `aether/${modelId}` } : {}),
    provider: {
      aether: {
        npm: provider.npm,
        name: provider.name,
        options: { baseURL: provider.options.baseURL, apiKey },
        models: provider.models,
      },
    },
  }
  const url = URL.createObjectURL(new Blob([`${JSON.stringify(contents, null, 2)}\n`], { type: 'application/json' }))
  const link = document.createElement('a')
  link.href = url
  link.download = 'opencode.json'
  link.click()
  // The browser may consume the blob asynchronously after the click.
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
