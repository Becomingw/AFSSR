<template>
  <section class="min-w-0 border-t py-5" aria-label="模型家族定义">
    <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
      <h2 class="text-base font-semibold">模型家族 <span class="text-xs font-normal text-muted-foreground">{{ Object.keys(modelValue).length }}/32</span></h2>
      <Button variant="outline" :disabled="disabled || Object.keys(modelValue).length >= 32" @click="add"><Plus class="h-4 w-4" />添加家族</Button>
    </div>
    <p v-if="catalogError" role="alert" class="mb-3 break-words text-sm text-destructive">{{ catalogError }}</p>
    <p v-if="!Object.keys(modelValue).length" class="py-3 text-sm text-muted-foreground">暂无模型家族</p>
    <div v-for="(family, id) in modelValue" :key="id" class="grid min-w-0 gap-3 border-b py-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,2fr)_auto]" :data-family-id="id">
      <div class="min-w-0 space-y-2">
        <label class="block text-sm" :for="`family-name-${id}`">显示名</label>
        <Input :id="`family-name-${id}`" :model-value="family.name" :disabled="disabled" aria-label="家族显示名" @update:model-value="update(id, { ...family, name: String($event) })" />
        <code class="block break-all text-xs text-muted-foreground">{{ id }}</code>
      </div>
      <div class="min-w-0 space-y-2">
        <span :id="`family-members-${id}`" class="block text-sm">公开模型成员 <span class="text-muted-foreground">{{ family.models.length }}/256</span></span>
        <MultiSelect :model-value="family.models" :options="options" :disabled="disabled || !!catalogError" :aria-labelledby="`family-members-${id}`" placeholder="选择公开模型" trigger-class="rounded-md" @update:model-value="update(id, { ...family, models: $event })" />
      </div>
      <div class="flex min-w-0 flex-col items-start gap-2 sm:max-w-40 sm:pt-7">
        <Button variant="ghost" size="icon" :title="references[id]?.length ? '被分配引用，无法删除' : '删除模型家族'" :aria-label="`删除家族 ${family.name || id}`" :disabled="disabled || !!references[id]?.length" @click="remove(id)"><Trash2 class="h-4 w-4" /></Button>
        <span v-if="references[id]?.length" class="break-words text-xs text-muted-foreground">引用：{{ references[id]!.join('、') }}</span>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Plus, Trash2 } from 'lucide-vue-next'
import { Button, Input } from '@/components/ui'
import MultiSelect from '@/components/common/MultiSelect.vue'
import type { ModelFamily } from '@/api/internal-access'

const props = defineProps<{
  modelValue: Record<string, ModelFamily>
  models: string[]
  references: Record<string, string[]>
  disabled: boolean
  catalogError: string
}>()
const emit = defineEmits<{ 'update:modelValue': [value: Record<string, ModelFamily>] }>()
const options = computed(() => props.models.map(name => ({ value: name, label: name })))
function add() {
  if (props.disabled || Object.keys(props.modelValue).length >= 32) return
  const id = `f-${crypto.randomUUID()}`
  emit('update:modelValue', { ...props.modelValue, [id]: { name: '', models: [] } })
}
function update(id: string, family: ModelFamily) {
  if (!props.disabled) emit('update:modelValue', { ...props.modelValue, [id]: family })
}
function remove(id: string) {
  if (props.disabled || props.references[id]?.length) return
  const next = { ...props.modelValue }
  delete next[id]
  emit('update:modelValue', next)
}
</script>
