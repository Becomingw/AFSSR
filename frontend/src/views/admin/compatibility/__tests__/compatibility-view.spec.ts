import { describe, expect, it } from 'vitest'

import type { CompatibilityCase, CompatibilityProfile, CompatibilityRun, CompatibilitySnapshot } from '@/api/compatibility'
import {
  caseCounts, caseTone, EXECUTED_CASES, kpis, overviewProfiles, relativeTime, sortByAttention, stateTone,
} from '../compatibility-view'

const config = { client: 'codex', client_version: '1', client_protocol: 'openai:responses', model: 'm', upstream_format: 'claude:messages', mapped_model: 'u' } as const
const profile = (id: string): CompatibilityProfile => ({ id, config: { ...config }, config_fingerprint: `fp-${id}` })
const allCases = (status: string): CompatibilityCase[] => EXECUTED_CASES.map(name => ({ case: name, status, reason: '' }))
const run = (id: string, profile_id: string, cases: CompatibilityCase[], created_at = 1): CompatibilityRun => ({
  id, profile_id, config: { ...config }, config_fingerprint: `fp-${profile_id}`, suite: 's', evidence: 'offline_synthetic', created_at, cases,
})
const snapshot = (profiles: CompatibilityProfile[], runs: { run: CompatibilityRun, stale: boolean }[]): CompatibilitySnapshot => ({
  schema_version: 1, revision: 1, can_run: true, suite: 's', fingerprint_scope: 'x', real_model_status: 'not_tested', real_client_status: 'not_tested', profiles, runs,
})

describe('compatibility view helpers', () => {
  it('uses the newest run per profile and marks missing or stale runs as needing a run', () => {
    const snap = snapshot([profile('a'), profile('b'), profile('c')], [
      { run: run('r2', 'a', allCases('passed')), stale: false },
      { run: run('r1', 'a', allCases('failed')), stale: false },
      { run: run('r3', 'b', allCases('passed')), stale: true },
    ])
    const overview = overviewProfiles(snap)
    expect(overview.map(item => [item.profile.id, item.state, item.needsRun])).toEqual([
      ['a', 'passed', false], ['b', 'passed', true], ['c', 'none', true],
    ])
    expect(kpis(overview)).toEqual({ total: 3, passed: 1, failed: 0, needsRun: 2 })
  })

  it('never reports unknown evidence or statuses as passed', () => {
    const odd = run('r', 'a', allCases('future_status'))
    expect(overviewProfiles(snapshot([profile('a')], [{ run: odd, stale: false }]))[0].state).not.toBe('passed')
    const foreign = { ...run('r', 'a', allCases('passed')), evidence: 'real' }
    expect(overviewProfiles(snapshot([profile('a')], [{ run: foreign, stale: false }]))[0].state).toBe('unknown')
    expect(caseTone('future_status')).toBe('muted')
    expect(stateTone('unknown')).toBe('warn')
  })

  it('puts failing and unrun configs before passing ones and keeps ties stable', () => {
    const snap = snapshot([profile('ok1'), profile('bad'), profile('none'), profile('ok2')], [
      { run: run('1', 'ok1', allCases('passed')), stale: false },
      { run: run('2', 'bad', allCases('failed')), stale: false },
      { run: run('3', 'ok2', allCases('passed')), stale: false },
    ])
    expect(sortByAttention(overviewProfiles(snap)).map(item => item.profile.id)).toEqual(['bad', 'none', 'ok1', 'ok2'])
  })

  it('counts only executed scenarios', () => {
    const cases = [...allCases('passed').slice(0, 7), { case: 'stream', status: 'failed', reason: '' }, { case: 'image', status: 'unsupported', reason: '' }, { case: 'real_model', status: 'not_tested', reason: '' }]
    expect(caseCounts(cases)).toEqual({ passed: 7, failed: 1, unsupported: 1, other: 0 })
  })

  it('formats relative time', () => {
    const now = 1_000_000_000_000
    expect(relativeTime(now / 1000 - 30, now)).toBe('刚刚')
    expect(relativeTime(now / 1000 - 600, now)).toBe('10 分钟前')
    expect(relativeTime(now / 1000 - 7200, now)).toBe('2 小时前')
    expect(relativeTime(now / 1000 - 3 * 86400, now)).toBe('3 天前')
    expect(relativeTime(Number.NaN, now)).toBe('未知时间')
  })
})
