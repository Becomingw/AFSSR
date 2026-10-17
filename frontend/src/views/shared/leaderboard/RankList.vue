<template>
  <section
    class="min-w-0"
    :aria-label="title || '排名列表'"
  >
    <h2
      v-if="title"
      class="mb-1 text-base font-semibold"
    >
      {{ title }}
    </h2>
    <p
      v-if="subtitle"
      class="mb-3 text-xs text-muted-foreground"
    >
      {{ subtitle }}
    </p>
    <div
      class="overflow-x-auto"
      :class="title && !subtitle ? 'mt-3' : ''"
    >
      <table class="w-full table-fixed text-sm">
        <thead class="border-y bg-muted/30 text-xs text-muted-foreground">
          <tr>
            <th class="w-14 p-3 text-left font-medium">
              排名
            </th><th class="p-3 text-left font-medium">
              {{ board.key === 'models' ? '模型' : '成员' }}
            </th><th class="w-32 p-3 text-right font-medium">
              {{ board.unit }}
            </th>
          </tr>
        </thead>
        <tbody class="divide-y">
          <tr
            v-for="item in board.items"
            :key="item.id"
          >
            <td
              class="p-3 font-medium tabular-nums"
              :class="item.rank <= 3 ? 'text-primary' : 'text-muted-foreground'"
            >
              {{ String(item.rank).padStart(2, '0') }}
            </td>
            <td class="break-words p-3 font-medium">
              {{ item.name }}
            </td>
            <td class="p-3 text-right tabular-nums">
              {{ format(item.value) }}
            </td>
          </tr>
          <tr v-if="!board.items.length">
            <td
              colspan="3"
              class="py-10 text-center text-sm text-muted-foreground"
            >
              {{ loading ? '正在汇总…' : '暂无可公开的排名数据' }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<script setup lang="ts">
import type { RankBoard } from '@/api/leaderboard'
const props = defineProps<{ title?: string; subtitle?: string; board: RankBoard; loading?: boolean }>()
function format(value: number | null) {
  if (value == null) return '未公开'
  if (props.board.unit === 'USD') return `$${value.toFixed(2)}`
  if (props.board.unit === '分钟') return `${Math.floor(value / 60)}h ${value % 60}m`
  return new Intl.NumberFormat('zh-CN').format(value)
}
</script>
