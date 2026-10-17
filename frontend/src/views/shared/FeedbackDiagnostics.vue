<template>
  <details class="feedback-diagnostic-disclosure">
    <summary>请求诊断<ChevronDown :size="15" /></summary>
    <div class="diagnostic-content">
      <label>请求 ID<Input
        :model-value="modelValue"
        aria-label="诊断请求 ID"
        :maxlength="128"
        @update:model-value="emit('update:modelValue', String($event))"
      /></label>
      <RequestDiagnosticPanel
        :request-id="modelValue.trim() || null"
        :admin="admin"
        hide-feedback
      />
      <Button
        v-if="canCreate && modelValue.trim()"
        variant="outline"
        @click="emit('create')"
      >
        带入新主题
      </Button>
    </div>
  </details>
</template>

<script setup lang="ts">
import { ChevronDown } from 'lucide-vue-next'
import { Button, Input } from '@/components/ui'
import RequestDiagnosticPanel from '@/features/usage/components/RequestDiagnosticPanel.vue'
defineProps<{ modelValue: string, admin: boolean, canCreate: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string], create: [] }>()
</script>

<style scoped>
.feedback-diagnostic-disclosure{border-top:1px solid var(--border);margin-top:24px;padding-top:16px;min-width:0}.feedback-diagnostic-disclosure summary{display:flex;justify-content:space-between;align-items:center;cursor:pointer;font-size:13px;font-weight:500;list-style:none}.feedback-diagnostic-disclosure summary::-webkit-details-marker{display:none}.feedback-diagnostic-disclosure summary:focus-visible{outline:2px solid var(--ring);outline-offset:4px}.feedback-diagnostic-disclosure[open] summary svg{transform:rotate(180deg)}.diagnostic-content{display:grid;gap:12px;margin-top:16px;min-width:0}.diagnostic-content label{display:grid;gap:8px;font-size:12px;color:var(--muted-foreground)}
</style>
