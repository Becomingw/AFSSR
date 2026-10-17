<template>
  <div class="quota-window">
    <div class="flex items-baseline justify-between gap-3">
      <h4 class="font-medium text-sm">
        {{ labels[window.period] }}
      </h4>
      <span class="text-sm tabular-nums shrink-0">{{ window.limit_usd === null ? '无上限' : `${percentage.toFixed(0)}% 已使用` }}</span>
    </div>
    <div
      v-if="window.limit_usd !== null"
      class="quota-track"
      role="progressbar"
      :aria-label="`${labels[window.period]}占用`"
      :aria-valuenow="percentage"
      :aria-valuemin="0"
      :aria-valuemax="100"
    >
      <div
        class="quota-fill"
        :class="[{ exhausted: percentage >= 100 }]"
        :style="{ width: `${percentage}%` }"
      />
    </div>
    <div class="flex flex-wrap justify-between gap-x-4 gap-y-1 text-xs text-muted-foreground mt-2">
      <span>已结算 {{ money(window.used_usd) }}<template v-if="window.reserved_usd > 0"> · 处理中 {{ money(window.reserved_usd) }}</template></span>
      <span v-if="window.limit_usd !== null">{{ money(window.remaining_usd ?? 0) }} 可用 / {{ money(window.limit_usd) }}</span>
    </div>
    <p class="mt-1.5 text-xs text-muted-foreground">
      {{ resetText }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { AccountWindow } from '@/api/account'
const props = defineProps<{ window: AccountWindow; timezone: string }>()
const labels = { five_hour: '最近 5 小时', daily: '每日额度', weekly: '每周额度', monthly: '每月额度' }
const money = (value: number) => new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 4 }).format(value)
const percentage = computed(() => props.window.limit_usd && props.window.limit_usd > 0 ? Math.max(0, Math.min(100, (props.window.used_usd + props.window.reserved_usd) / props.window.limit_usd * 100)) : 0)
const resetText = computed(() => {
  if (props.window.period === 'five_hour') return '滚动窗口，用量随时间逐步释放'
  if (!props.window.resets_at) return '未记录重置时间'
  try {
    return `${new Intl.DateTimeFormat('zh-CN', { timeZone: props.timezone, month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(props.window.resets_at * 1000)} 重置 · ${props.timezone}`
  } catch { return '重置时间暂不可用' }
})
</script>

<style scoped>
.quota-window { min-width: 0; padding: 1.125rem 0; }
.quota-track { height: 7px; margin-top: .8rem; border-radius: 4px; overflow: hidden; background: var(--muted); }
.quota-fill { height: 100%; background: var(--foreground); border-radius: 4px; transition: width .2s; }
.quota-fill.exhausted { background: var(--destructive); }
</style>
