import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const read = (name: string) => readFileSync(resolve(__dirname, '..', name), 'utf8')

describe('OpenCode Go is a key-only preset', () => {
  it('hides endpoint editing and conversion switches in the provider header', () => {
    const source = read('ProviderDetailHeader.vue')
    expect(source).toContain("props.provider.provider_type === 'opencode_go'")
    expect(source.match(/v-if="!isGoPreset"/g)).toHaveLength(2)
    expect(source).toContain('!isGoPreset && $emit(\'editEndpoint\'')
  })

  it('does not ask for API formats on a Go key and imports the catalog after saving', () => {
    const source = read('KeyFormDialog.vue')
    expect(source).toContain('v-if="isGoPreset"')
    expect(source).toContain('v-else-if="visibleApiFormats.length > 0"')
    expect(source).toContain('if (isGoPreset.value) await importGoCatalog(createdKey.id)')
    expect(source).toContain('importModelsFromUpstream(props.providerId, ids)')
  })
})
