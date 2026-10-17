import { describe, expect, it } from 'vitest'
import type { CandidateRecord, RequestTrace } from '@/api/requestTrace'
import {
  boundedJson,
  failedAttempts,
  upstreamErrorView,
  upstreamRequestAvailability,
} from '../upstream-failure'

function candidate(overrides: Partial<CandidateRecord> = {}): CandidateRecord {
  return {
    id: 'c1',
    request_id: 'r1',
    candidate_index: 0,
    retry_index: 0,
    status: 'failed',
    is_cached: false,
    created_at: '2026-10-17T00:00:00Z',
    ...overrides,
  }
}

const trace = (candidates: CandidateRecord[]): RequestTrace => ({
  request_id: 'r1',
  total_candidates: candidates.length,
  final_status: 'failed',
  total_latency_ms: 5,
  candidates,
})

describe('upstream failure helpers', () => {
  it('only treats attempts that reached an upstream and failed as evidence', () => {
    const attempts = failedAttempts(trace([
      candidate({ id: 'skipped', status: 'skipped', skip_reason: 'no_key' }),
      candidate({ id: 'ok', status: 'success', status_code: 200 }),
      candidate({ id: 'bare', status: 'failed' }),
      candidate({ id: 'coded', status: 'failed', status_code: 400 }),
      candidate({ id: 'interrupted', status: 'stream_interrupted', error_message: 'boom' }),
    ]))
    expect(attempts.map(a => a.id)).toEqual(['coded', 'interrupted'])
    expect(failedAttempts(null)).toEqual([])
  })

  it('counts an in-band stream failure recorded on a successful attempt, and falls back to the kept body', () => {
    const inBand = candidate({
      id: 'inband',
      status: 'success',
      status_code: 200,
      error_message: 'overloaded',
      extra_data: { upstream_response: { status_code: 200, body_state: 'reference' } },
    })
    expect(failedAttempts(trace([inBand, candidate({ id: 'ok', status: 'success', status_code: 200 })])).map(a => a.id)).toEqual(['inband'])
    const view = upstreamErrorView(inBand, { error: { message: 'overloaded' } })
    expect(view.statusCode).toBe(200)
    expect(view.body).toEqual({ error: { message: 'overloaded' } })
    expect(upstreamErrorView(inBand).body).toBeUndefined()
  })

  it('extracts status, useful headers, body, formats and the gateway decision', () => {
    const view = upstreamErrorView(candidate({
      candidate_index: 1,
      retry_index: 1,
      provider_name: 'Acme',
      status_code: 400,
      error_message: 'unsupported parameter',
      extra_data: {
        client_api_format: 'openai:responses',
        provider_api_format: 'openai:chat',
        upstream_url: 'http://127.0.0.1:1/',
        upstream_response: {
          status_code: 429,
          headers: { 'retry-after': '7', 'content-length': '12', 'x-ratelimit-remaining': '0', date: 'now' },
          body: { error: { message: 'slow down' } },
        },
        error_flow: { decision: 'retry_next_candidate', classification: 'retry_upstream_failure', retryable: true },
      },
    }))
    expect(view.attemptLabel).toBe('候选 2 · 重试 1')
    expect(view.statusCode).toBe(429)
    expect(view.headers).toEqual([['retry-after', '7'], ['x-ratelimit-remaining', '0']])
    expect(view.body).toEqual({ error: { message: 'slow down' } })
    expect(view.formats).toBe('openai:responses → openai:chat')
    expect(view.decision).toBe('retry_next_candidate')
    expect(view.retryable).toBe(true)
  })

  it('falls back to the recorded message and keeps text bodies as text', () => {
    const view = upstreamErrorView(candidate({
      status_code: 502,
      error_message: 'bad gateway',
      extra_data: { upstream_response: { status_code: 502, body: '<html>502</html>' } },
    }))
    expect(view.bodyIsText).toBe(true)
    expect(view.message).toBe('bad gateway')
    expect(view.formats).toBeNull()
    expect(upstreamErrorView(candidate({ status_code: 500 })).headers).toEqual([])
  })

  it('bounds large payloads and explains an absent upstream request', () => {
    const big = boundedJson({ text: 'x'.repeat(100) }, 20)
    expect(big.truncated).toBe(true)
    expect(big.text).toHaveLength(20)
    expect(boundedJson('plain')).toEqual({ text: 'plain', truncated: false })
    expect(upstreamRequestAvailability({ a: 1 }, false, 'disabled')).toBe('present')
    expect(upstreamRequestAvailability(null, true, null)).toBe('present')
    expect(upstreamRequestAvailability(null, false, 'disabled')).toBe('disabled')
    expect(upstreamRequestAvailability(null, false, null)).toBe('missing')
  })
})
