import { describe, expect, it } from 'vitest'

import { modelsNeedingProviderLink } from '../modelLinking'

describe('modelsNeedingProviderLink', () => {
  it('returns only selected models that are not linked to the provider yet', () => {
    expect(
      modelsNeedingProviderLink(['gpt-5', 'claude-x', 'gemini-y'], ['gpt-5']),
    ).toEqual(['claude-x', 'gemini-y'])
  })

  it('drops blanks and duplicates but keeps the selection order', () => {
    expect(modelsNeedingProviderLink([' b ', 'a', 'b', '', '  ', 'a'], [])).toEqual(['b', 'a'])
  })

  it('never asks to unlink: models missing from the selection are simply ignored', () => {
    expect(modelsNeedingProviderLink([], ['gpt-5', 'claude-x'])).toEqual([])
  })

  it('treats the provider model names as exact matches', () => {
    expect(modelsNeedingProviderLink(['GPT-5'], ['gpt-5'])).toEqual(['GPT-5'])
  })
})
