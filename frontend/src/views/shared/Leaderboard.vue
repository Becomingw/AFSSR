<template>
  <PageContainer>
    <PageHeader
      class="[&>div:first-child]:min-w-[180px]"
      title="使用排行榜"
      :icon="Trophy"
    >
      <template #actions>
        <select
          v-model="preset"
          aria-label="统计区间"
          class="h-9 rounded-md border bg-background px-3 text-sm"
        >
          <option value="this_week">
            本周
          </option><option value="last_week">
            上周
          </option>
          <option value="last7days">
            最近 7 天
          </option><option value="last30days">
            最近 30 天
          </option>
        </select>
        <select
          v-model.number="offset"
          aria-label="统计时区"
          class="h-9 rounded-md border bg-background px-2 text-sm"
        >
          <option :value="480">
            UTC+8
          </option><option :value="0">
            UTC
          </option><option :value="540">
            UTC+9
          </option>
        </select>
        <Button
          variant="outline"
          size="icon"
          title="刷新排行榜"
          :disabled="loading"
          @click="load"
        >
          <RefreshCw
            class="h-4 w-4"
            :class="{ 'animate-spin': loading }"
          />
        </Button>
      </template>
    </PageHeader>

    <div
      v-if="error"
      role="alert"
      class="flex flex-wrap items-center justify-between gap-3 border-y border-destructive/30 py-4 text-sm text-destructive"
    >
      <span>{{ error }}</span><Button
        variant="outline"
        @click="load"
      >
        重试
      </Button>
    </div>
    <p
      v-if="loading"
      role="status"
      class="text-sm text-muted-foreground"
    >
      正在汇总排行榜…
    </p>

    <template v-if="report">
      <div class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
        <span>{{ report.start_date }} 至 {{ report.end_date }} · {{ report.masked ? '脱敏视图' : '管理视图' }}</span>
        <span>{{ updatedAt }} 更新</span>
      </div>
      <dl class="grid grid-cols-2 gap-x-6 gap-y-5 border-y py-5 lg:grid-cols-4">
        <div
          v-for="item in totals"
          :key="item.label"
          class="min-w-0 border-l-2 pl-3"
          :class="item.color"
        >
          <dt class="text-xs text-muted-foreground">
            {{ item.label }}
          </dt>
          <dd class="mt-1 break-words text-2xl font-semibold tabular-nums">
            {{ item.value }}
          </dd>
        </div>
      </dl>
      <p
        v-if="report.masked"
        class="text-xs text-muted-foreground"
      >
        默认显示用户名，选择匿名的成员显示为匿名代号（可在个人设置中开启）。金额仅显示排名。少于 3 位用户时隐藏用户榜单；精确调用时间不公开。
      </p>
      <div
        role="tablist"
        aria-label="排行榜分类"
        class="flex flex-wrap gap-1 border-b"
      >
        <button
          v-for="tab in tabs"
          :id="`rank-tab-${tab.key}`"
          :key="tab.key"
          type="button"
          role="tab"
          :aria-selected="view === tab.key"
          aria-controls="rank-panel"
          class="flex items-center gap-2 border-b-2 px-4 py-3 text-sm"
          :class="view === tab.key ? 'border-primary font-medium text-foreground' : 'border-transparent text-muted-foreground'"
          @click="view = tab.key"
          @keydown="moveTab($event, tab.key)"
        >
          <component
            :is="tab.icon"
            class="h-4 w-4"
          />{{ tab.label }}
        </button>
      </div>
      <div
        id="rank-panel"
        role="tabpanel"
        :aria-labelledby="`rank-tab-${view}`"
        :class="loading ? 'pointer-events-none opacity-60' : ''"
        :aria-busy="loading"
      >
        <div
          v-if="view === 'usage'"
          class="grid min-w-0 gap-8 xl:grid-cols-2"
        >
          <RankList
            title="最受欢迎的模型"
            :board="board('models')"
            :loading="loading"
          />
          <section class="min-w-0">
            <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
              <h2 class="text-base font-semibold">
                用户用量
              </h2>
              <select
                v-model="userMetric"
                aria-label="用户排名指标"
                class="h-9 rounded-md border bg-background px-2 text-sm"
              >
                <option value="requests">
                  调用次数
                </option><option value="tokens">
                  Token 用量
                </option><option value="cost">
                  使用金额
                </option>
              </select>
            </div>
            <RankList
              :board="board(userMetric)"
              :loading="loading"
            />
          </section>
        </div>
        <div
          v-else-if="view === 'activity'"
          class="space-y-8"
        >
          <div class="flex items-start gap-2 text-xs leading-5 text-muted-foreground">
            <Info class="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <p>活跃时长为成功调用覆盖的 5 分钟时间桶累计值，并非实际工时。自动化调用也会计入；时长、时段和早鸟仅统计仍保留的明细。</p>
          </div>
          <div class="grid min-w-0 gap-8 xl:grid-cols-2">
            <RankList
              :title="`${report.end_date} 活跃时长`"
              :board="board('daily_active')"
            />
            <RankList
              title="自然周活跃时长"
              :subtitle="`截至 ${report.end_date}，周一开始`"
              :board="board('weekly_active')"
            />
            <RankList
              title="活跃天数"
              :board="board('active_days')"
            />
            <RankList
              title="最长连续使用"
              subtitle="所选区间内的连续活跃天数"
              :board="board('streak')"
            />
          </div>
        </div>
        <div
          v-else
          class="space-y-8"
        >
          <section class="min-w-0 space-y-4">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h2 class="text-base font-semibold">
                24 小时调用分布
              </h2><span class="text-xs text-muted-foreground">成功调用 · 所选时区</span>
            </div>
            <div class="overflow-x-auto">
              <div
                class="grid min-w-[600px] grid-cols-[repeat(24,minmax(0,1fr))] items-end gap-1 border-b pb-2"
                role="group"
                aria-label="24 小时成功调用量柱状图"
              >
                <button
                  v-for="hour in report.hours"
                  :key="hour.hour"
                  class="group flex h-36 flex-col justify-end gap-2"
                  type="button"
                  :aria-label="`${hour.hour} 时，${number(hour.requests)} 次调用`"
                  :aria-pressed="selectedHour === hour.hour"
                  :title="`${hour.hour}:00 · ${number(hour.requests)} 次`"
                  @click="selectedHour = hour.hour"
                >
                  <span
                    class="block w-full rounded-t-sm transition-colors"
                    :class="selectedHour === hour.hour ? 'bg-primary' : 'bg-emerald-500/40 group-hover:bg-emerald-500/70'"
                    :style="{ height: `${hour.requests ? Math.max(3, hour.requests / maxHour * 110) : 1}px` }"
                  />
                  <span class="text-center text-[10px] tabular-nums text-muted-foreground">{{ hour.hour }}</span>
                </button>
              </div>
            </div>
            <RankList
              :title="`${String(selectedHour).padStart(2, '0')}:00–${String(selectedHour + 1).padStart(2, '0')}:00 主要活跃成员`"
              :board="{ key: 'hour', unit: '次', items: report.hours.find(h => h.hour === selectedHour)?.leaders || [] }"
            />
          </section>
          <div class="grid min-w-0 gap-8 xl:grid-cols-2">
            <section class="min-w-0">
              <div class="mb-3 flex items-center gap-2">
                <Sunrise class="h-4 w-4 text-amber-600" /><h2 class="text-base font-semibold">
                  每日早鸟之王
                </h2>
              </div>
              <div class="overflow-x-auto">
                <table class="w-full text-sm">
                  <thead class="border-y bg-muted/30 text-left text-xs text-muted-foreground">
                    <tr>
                      <th class="p-3 font-medium">
                        日期
                      </th><th class="p-3 font-medium">
                        最早成功调用
                      </th><th
                        v-if="!report.masked"
                        class="p-3 text-right font-medium"
                      >
                        时间
                      </th>
                    </tr>
                  </thead>
                  <tbody class="divide-y">
                    <tr
                      v-for="day in report.early_birds"
                      :key="day.date"
                    >
                      <td class="whitespace-nowrap p-3 tabular-nums">
                        {{ day.date }}
                      </td><td class="max-w-56 break-words p-3">
                        {{ day.winners.map(w => w.name).join('、') || '未公开' }}
                      </td><td
                        v-if="!report.masked"
                        class="whitespace-nowrap p-3 text-right tabular-nums"
                      >
                        {{ day.time }}
                      </td>
                    </tr>
                    <tr v-if="!report.early_birds.length">
                      <td
                        :colspan="report.masked ? 2 : 3"
                        class="py-10 text-center text-muted-foreground"
                      >
                        暂无成功调用记录
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </section>
            <RankList
              title="早鸟夺冠天数"
              subtitle="每天 06:00 起的首次成功调用，同秒并列"
              :board="board('early_wins')"
            />
            <RankList
              title="熬夜之王"
              subtitle="22:00 至次日 06:00，区间内累计估算活跃时长"
              :board="board('night_active')"
            />
            <RankList
              title="熬夜夺冠晚数"
              subtitle="每晚活跃时长最多者夺冠，相同时并列"
              :board="board('night_wins')"
            />
            <section class="min-w-0 xl:col-span-2">
              <h2 class="mb-1 text-base font-semibold">
                每日熬夜之王
              </h2>
              <p class="mb-3 text-xs text-muted-foreground">
                日期为 22:00 开始的那一天；进行中的夜晚仅累计已发生的调用。
              </p>
              <div class="overflow-x-auto">
                <table class="w-full text-sm">
                  <thead class="border-y bg-muted/30 text-left text-xs text-muted-foreground">
                    <tr>
                      <th class="p-3 font-medium">
                        夜晚开始日
                      </th><th class="p-3 font-medium">
                        成员
                      </th><th class="p-3 text-right font-medium">
                        活跃时长
                      </th>
                    </tr>
                  </thead>
                  <tbody class="divide-y">
                    <tr
                      v-for="night in report.night_owls"
                      :key="night.date"
                    >
                      <td class="whitespace-nowrap p-3 tabular-nums">
                        {{ night.date }}
                      </td><td class="max-w-56 break-words p-3">
                        {{ night.winners.map(w => w.name).join('、') || '未公开' }}
                      </td><td class="whitespace-nowrap p-3 text-right tabular-nums">
                        {{ night.winners.length && night.winners[0]?.value != null ? `${night.winners[0].value} 分钟` : '未公开' }}
                      </td>
                    </tr>
                    <tr v-if="!report.night_owls.length">
                      <td
                        colspan="3"
                        class="py-10 text-center text-muted-foreground"
                      >
                        暂无夜间成功调用记录
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </section>
          </div>
        </div>
      </div>
    </template>
  </PageContainer>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { ChartNoAxesColumn, Clock3, Info, RefreshCw, Sunrise, Trophy } from 'lucide-vue-next'
import PageContainer from '@/components/layout/PageContainer.vue'
import PageHeader from '@/components/layout/PageHeader.vue'
import { Button } from '@/components/ui'
import RankList from './leaderboard/RankList.vue'
import { getLeaderboard, type LeaderboardReport, type RankBoard } from '@/api/leaderboard'

const route = useRoute()
const admin = computed(() => route.path.startsWith('/admin/'))
const preset = ref('this_week'), offset = ref(480), view = ref('usage'), userMetric = ref('requests'), selectedHour = ref(9)
const loading = ref(false), error = ref(''), report = ref<LeaderboardReport>()
const tabs = [{ key: 'usage', label: '用量排行', icon: ChartNoAxesColumn }, { key: 'activity', label: '活跃排行', icon: Clock3 }, { key: 'time', label: '时段与荣誉', icon: Sunrise }]
let generation = 0, controller: AbortController | undefined
const number = (v: number) => new Intl.NumberFormat('zh-CN', { notation: 'compact', maximumFractionDigits: 1 }).format(v)
const totals = computed(() => report.value ? [
  { label: '调用总次数', value: number(report.value.requests), color: 'border-emerald-500' },
  { label: 'Token 总量', value: number(report.value.tokens), color: 'border-sky-500' },
  { label: '参与用户', value: number(report.value.users), color: 'border-amber-500' },
  { label: report.value.masked ? '使用模型' : '使用总金额', value: report.value.cost == null ? number(report.value.models) : `$${report.value.cost.toFixed(2)}`, color: 'border-rose-500' },
] : [])
const updatedAt = computed(() => report.value ? new Date(report.value.generated_at * 1000).toLocaleTimeString() : '')
const maxHour = computed(() => Math.max(1, ...report.value?.hours.map(h => h.requests) || []))
function board(key: string): RankBoard { return report.value?.boards.find(b => b.key === key) || { key, unit: '', items: [] } }
function moveTab(event: KeyboardEvent, key: string) {
  if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  const index = tabs.findIndex(t => t.key === key)
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length
  const target = tabs[next]
  if (!target) return
  view.value = target.key
  document.getElementById(`rank-tab-${view.value}`)?.focus()
}
async function load() {
  const id = ++generation
  controller?.abort(); controller = new AbortController()
  report.value = undefined; loading.value = true; error.value = ''
  try { const value = await getLeaderboard(admin.value, preset.value, offset.value, controller.signal); if (id === generation) report.value = value }
  catch { if (id === generation) error.value = '排行榜暂时不可用，请稍后重试；没有用量存储时不会显示模拟统计。' }
  finally { if (id === generation) loading.value = false }
}
watch([preset, offset, admin], load)
onMounted(load)
onBeforeUnmount(() => { generation++; controller?.abort() })
</script>
