import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

describe('OpenCode Go provider entry', () => {
  it('shows the dedicated provider in both create and edit selectors', () => {
    const source = readFileSync(resolve(process.cwd(), 'src/features/providers/components/ProviderFormDialog.vue'), 'utf8')
    expect(source.match(/<SelectItem value="opencode_go">/g)).toHaveLength(2)
    expect(source).toContain('OpenCode Go')
  })

  it('supports API key batch imports rather than OAuth accounts', () => {
    const source = readFileSync(resolve(process.cwd(), 'src/features/providers/components/ProviderDetailDrawer.vue'), 'utf8')
    expect(source.match(/\['custom', 'opencode_go'\]\.includes/g)).toHaveLength(2)
  })
})
