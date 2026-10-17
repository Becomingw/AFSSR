import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { compileStyle, parse } from '@vue/compiler-sfc'
import { describe, expect, it } from 'vitest'
import postcss from 'postcss'

describe('homepage dark theme scope', () => {
  it.each([
    ['Home.vue', '.arca-home'],
    ['HarnessCarousel.vue', '.monochrome'],
    ['ProtocolFeature.vue', '.is-monochrome'],
  ])('keeps %s dark rules on the intended elements rather than the whole document', (file, target) => {
    const filename = resolve(process.cwd(), 'src/views/public', file)
    const { descriptor } = parse(readFileSync(filename, 'utf8'))
    const source = descriptor.styles[0]?.content ?? ''
    const result = compileStyle({ filename, source, id: 'data-v-scope-test', scoped: true })
    expect(result.errors).toHaveLength(0)
    const darkSelectors: string[] = []
    postcss.parse(result.code).walkRules(rule => {
      if (rule.selector.includes('.dark')) darkSelectors.push(rule.selector)
    })
    expect(darkSelectors.length).toBeGreaterThan(0)
    expect(darkSelectors.every(selector => selector.includes(`${target}[data-v-scope-test]`))).toBe(true)
    expect(darkSelectors).not.toContain('.dark')
  })
})

describe('feature option hit targets', () => {
  it.each(['ProtocolFeature.vue', 'EntitlementsFeature.vue'])('keeps native radio hit targets inside the full %s option', (file) => {
    const filename = resolve(process.cwd(), 'src/views/public', file)
    const { descriptor } = parse(readFileSync(filename, 'utf8'))
    const result = compileStyle({ filename, source: descriptor.styles[0]?.content ?? '', id: 'data-v-hit-test', scoped: true })
    const declarations: Record<string, string> = {}
    postcss.parse(result.code).walkRules(rule => {
      if (rule.selector.endsWith('input[data-v-hit-test]')) {
        rule.walkDecls(declaration => { declarations[declaration.prop] = declaration.value })
      }
    })
    expect(declarations.inset).toBe('0')
    expect(declarations.width).toBe('100%')
    expect(declarations.height).toBe('100%')
  })
})

describe('protocol endpoint wrapping', () => {
  it('overrides global code whitespace so Gemini paths fit narrow screens', () => {
    const filename = resolve(process.cwd(), 'src/views/public/ProtocolFeature.vue')
    const { descriptor } = parse(readFileSync(filename, 'utf8'))
    const result = compileStyle({ filename, source: descriptor.styles[0]?.content ?? '', id: 'data-v-wrap-test', scoped: true })
    const declarations: Record<string, string> = {}
    let overridesGlobalWhitespace = false
    postcss.parse(result.code).walkRules(rule => {
      if (rule.selector === '.protocol-endpoints code[data-v-wrap-test]') {
        rule.walkDecls(declaration => {
          declarations[declaration.prop] = declaration.value
          if (declaration.prop === 'white-space') overridesGlobalWhitespace = declaration.important
        })
      }
    })
    expect(declarations['white-space']).toBe('normal')
    expect(declarations['overflow-wrap']).toBe('anywhere')
    expect(declarations['min-width']).toBe('0')
    expect(overridesGlobalWhitespace).toBe(true)
  })
})
