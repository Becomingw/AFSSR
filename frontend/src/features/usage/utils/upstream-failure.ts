import type { CandidateRecord, RequestTrace } from '@/api/requestTrace'

type JsonObject = Record<string, unknown>

const asObject = (value: unknown): JsonObject =>
  value !== null && typeof value === 'object' && !Array.isArray(value) ? value as JsonObject : {}

/** Headers worth reading when an upstream rejects a request; everything else is noise here. */
const INTERESTING_HEADERS = /^(retry-after|content-type|x-request-id|request-id|cf-ray|x-ratelimit-.*|anthropic-ratelimit-.*|openai-.*|x-should-retry)$/i

export const MAX_PANEL_JSON_CHARS = 40_000

export interface UpstreamErrorView {
  attemptLabel: string
  providerName: string
  statusCode: number | null
  headers: Array<[string, string]>
  body: unknown
  bodyIsText: boolean
  bodyState: string | null
  upstreamUrl: string | null
  formats: string | null
  decision: string | null
  classification: string | null
  retryable: boolean | null
  message: string
}

/**
 * Attempts that actually reached an upstream and carry a failure (skipped/unused candidates have no
 * evidence). A stream that fails in-band after an HTTP 200 is recorded on a "success" attempt that
 * only has an error message, so that counts too.
 */
export function failedAttempts(trace: RequestTrace | null | undefined): CandidateRecord[] {
  return (trace?.candidates ?? []).filter((candidate) => {
    const extra = asObject(candidate.extra_data)
    if (candidate.status === 'success') return Boolean(candidate.error_message)
    if (candidate.status !== 'failed' && candidate.status !== 'stream_interrupted') return false
    return Boolean(extra.upstream_response) || Boolean(candidate.error_message) || candidate.status_code != null
  })
}

/**
 * `fallbackBody` is the response body kept with the request record; it is used when the attempt
 * itself holds no upstream body (in-band stream failures keep the error only there).
 */
export function upstreamErrorView(candidate: CandidateRecord, fallbackBody?: unknown): UpstreamErrorView {
  const extra = asObject(candidate.extra_data)
  const response = asObject(extra.upstream_response)
  const flow = asObject(extra.error_flow)
  const headers = Object.entries(asObject(response.headers))
    .filter(([name]) => INTERESTING_HEADERS.test(name))
    .map(([name, value]) => [name, String(value)] as [string, string])
  const client = typeof extra.client_api_format === 'string' ? extra.client_api_format : ''
  const provider = typeof extra.provider_api_format === 'string' ? extra.provider_api_format : ''
  const body = response.body ?? fallbackBody ?? undefined
  return {
    attemptLabel: `候选 ${candidate.candidate_index + 1}${candidate.retry_index ? ` · 重试 ${candidate.retry_index}` : ''}`,
    providerName: candidate.provider_name || candidate.provider_id || '未知提供商',
    statusCode: typeof response.status_code === 'number' ? response.status_code : candidate.status_code ?? null,
    headers,
    body,
    bodyIsText: typeof body === 'string',
    bodyState: typeof response.body_state === 'string' ? response.body_state : null,
    upstreamUrl: typeof extra.upstream_url === 'string' ? extra.upstream_url : null,
    formats: client && provider ? (client === provider ? client : `${client} → ${provider}`) : null,
    decision: typeof flow.decision === 'string' ? flow.decision : null,
    classification: typeof flow.classification === 'string' ? flow.classification : null,
    retryable: typeof flow.retryable === 'boolean' ? flow.retryable : null,
    message: candidate.error_message || (typeof flow.message === 'string' ? flow.message : ''),
  }
}

/** Pretty-prints a value for the panel, bounded so a huge conversation cannot freeze the drawer. */
export function boundedJson(value: unknown, limit = MAX_PANEL_JSON_CHARS): { text: string, truncated: boolean } {
  const text = typeof value === 'string' ? value : JSON.stringify(value, null, 2) ?? ''
  return text.length > limit ? { text: text.slice(0, limit), truncated: true } : { text, truncated: false }
}

export type UpstreamRequestAvailability = 'present' | 'disabled' | 'missing'

/** Why the request sent upstream may be absent, so the panel can say how to get it next time. */
export function upstreamRequestAvailability(
  body: unknown,
  hasBody: boolean | undefined,
  bodyState: string | null | undefined,
): UpstreamRequestAvailability {
  if (body != null || hasBody) return 'present'
  return bodyState === 'disabled' ? 'disabled' : 'missing'
}
