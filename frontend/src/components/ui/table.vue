<script setup lang="ts">
import type { ClassValue } from 'clsx'
import { cn } from '@/lib/utils'
import { computed } from 'vue'

interface Props {
  class?: ClassValue
}

const props = defineProps<Props>()

const tableClass = computed(() =>
  cn('w-full caption-bottom text-sm', props.class)
)
</script>

<template>
  <div data-ui="table-scroll" class="relative min-w-0 w-full overflow-auto">
    <table :class="tableClass">
      <slot />
    </table>
  </div>
</template>
