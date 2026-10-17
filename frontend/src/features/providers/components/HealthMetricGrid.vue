<template>
  <div class="health-metric-grid grid min-w-0 w-full gap-2">
    <div
      v-for="metric in metrics"
      :key="metric.label"
      class="min-w-0 rounded-lg border border-border/40 bg-muted/20 px-3 py-2"
    >
      <div class="text-[11px] leading-tight text-muted-foreground">
        {{ metric.label }}
      </div>
      <div
        class="mt-1 break-words text-sm font-semibold tabular-nums [overflow-wrap:anywhere]"
        :class="metric.valueClass"
      >
        {{ metric.value }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import {
  formatAvailability,
  formatMs,
  formatTps,
  getAvailabilityClass
} from './health-monitor-utils'

const props = defineProps<{
  avgLatencyMs?: number | null
  avgFirstByteMs?: number | null
  avgTps?: number | null
  totalAttempts: number
  successRate: number
}>()

const availabilityItem = computed(() => ({
  total_attempts: props.totalAttempts,
  success_rate: props.successRate
}))

const metrics = computed(() => [
  {
    label: '平均耗时',
    value: formatMs(props.avgLatencyMs),
    valueClass: ''
  },
  {
    label: '平均TTFB',
    value: formatMs(props.avgFirstByteMs),
    valueClass: ''
  },
  {
    label: '平均速度',
    value: formatTps(props.avgTps),
    valueClass: ''
  },
  {
    label: '可用率',
    value: formatAvailability(availabilityItem.value),
    valueClass: getAvailabilityClass(availabilityItem.value)
  }
])
</script>

<style scoped>
.health-metric-grid {
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 7rem), 1fr));
}
</style>
