<script setup lang="ts">
import type { ClassValue } from 'clsx'
import { SelectTrigger as SelectTriggerPrimitive } from 'radix-vue'
import { ChevronDown } from 'lucide-vue-next'
import { cn } from '@/lib/utils'
import { computed } from 'vue'

interface Props {
  class?: ClassValue
  disabled?: boolean
}

const props = defineProps<Props>()

const triggerClass = computed(() =>
  cn(
    'flex h-10 w-full min-w-0 items-center justify-between gap-2 rounded-md border border-border bg-card px-3 py-2 text-left text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/30 focus:border-primary disabled:cursor-not-allowed disabled:opacity-60 text-foreground cursor-pointer transition-colors',
    props.class
  )
)
</script>

<template>
  <SelectTriggerPrimitive
    v-bind="$attrs"
    :class="triggerClass"
    :disabled="disabled"
  >
    <span class="min-w-0 flex-1 truncate">
      <slot />
    </span>
    <ChevronDown class="h-4 w-4 opacity-50 pointer-events-none flex-shrink-0" />
  </SelectTriggerPrimitive>
</template>
