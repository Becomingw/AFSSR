<template>
  <section
    class="token-activity"
    aria-labelledby="token-activity-title"
  >
    <header class="activity-header">
      <h2 id="token-activity-title">
        Token 活动
      </h2>
      <div class="activity-controls">
        <div
          class="activity-modes"
          role="group"
          aria-label="Token 活动统计方式"
        >
          <button
            v-for="item in modes"
            :key="item.value"
            type="button"
            :aria-pressed="mode === item.value"
            @click="mode = item.value"
          >
            {{ item.label }}
          </button>
        </div>
        <button
          type="button"
          class="activity-refresh"
          aria-label="刷新 Token 活动"
          title="刷新 Token 活动"
          :disabled="loading"
          @click="load(true)"
        >
          <RefreshCw
            :size="14"
            :class="{ 'animate-spin': loading }"
          />
        </button>
      </div>
    </header>
    <p
      v-if="error"
      role="alert"
      class="activity-message"
    >
      Token 活动暂不可用。<button
        type="button"
        @click="load(true)"
      >
        重试
      </button>
    </p>
    <p
      v-else-if="!history"
      role="status"
      class="activity-message"
    >
      正在加载 Token 活动…
    </p>
    <template v-else>
      <div
        ref="scroller"
        class="activity-scroll"
        :aria-busy="loading"
      >
        <div
          ref="chart"
          class="activity-chart"
          :style="{ '--weeks': history.weeks.length }"
          aria-label="最近一年 Token 活动图"
        >
          <div
            class="activity-columns"
            :class="{ 'weekly-columns': mode === 'weekly' }"
          >
            <div
              v-for="(week, weekIndex) in history.weeks"
              :key="week.start"
              class="activity-week"
            >
              <button
                v-if="mode === 'weekly'"
                type="button"
                class="activity-cell weekly-bar"
                :data-level="tokenLevel(week.tokens, maximum)"
                :style="{ height: `${maximum ? Math.max(6, week.tokens / maximum * 100) : 6}%` }"
                :data-point="weekIndex"
                :tabindex="weekIndex === focused ? 0 : -1"
                :aria-label="pointLabel(weekIndex)"
                :aria-describedby="tooltip?.index === weekIndex ? tooltipId : undefined"
                @pointerenter="show(weekIndex, $event)"
                @pointerleave="hide"
                @focus="show(weekIndex, $event)"
                @blur="hide"
                @click="show(weekIndex, $event)"
                @keydown="navigate($event, weekIndex)"
              />
              <template v-else>
                <template
                  v-for="(day, dayIndex) in week.days"
                  :key="day?.date ?? `padding-${dayIndex}`"
                >
                  <button
                    v-if="day"
                    type="button"
                    class="activity-cell"
                    :data-level="tokenLevel(mode === 'cumulative' ? day.cumulative : day.tokens, maximum)"
                    :data-point="dayOffset(weekIndex, dayIndex)"
                    :tabindex="dayOffset(weekIndex, dayIndex) === focused ? 0 : -1"
                    :aria-label="pointLabel(dayOffset(weekIndex, dayIndex))"
                    :aria-describedby="tooltip?.index === dayOffset(weekIndex, dayIndex) ? tooltipId : undefined"
                    @pointerenter="show(dayOffset(weekIndex, dayIndex), $event)"
                    @pointerleave="hide"
                    @focus="show(dayOffset(weekIndex, dayIndex), $event)"
                    @blur="hide"
                    @click="show(dayOffset(weekIndex, dayIndex), $event)"
                    @keydown="navigate($event, dayOffset(weekIndex, dayIndex))"
                  />
                  <span
                    v-else
                    class="activity-padding"
                    aria-hidden="true"
                  />
                </template>
              </template>
            </div>
          </div>
          <div
            class="activity-months"
            aria-hidden="true"
          >
            <span
              v-for="week in history.weeks"
              :key="week.start"
            >{{ week.month }}</span>
          </div>
        </div>
      </div>
      <footer class="activity-footer">
        <p>
          {{ history.start }} – {{ history.end }} · UTC<template v-if="mode === 'cumulative'">
            · 区间内累计
          </template><br><span>{{ number(history.total) }} Token · 有用量 {{ history.activeDays }} 天</span>
        </p>
        <div
          class="activity-legend"
          aria-label="颜色越深，Token 用量越多"
        >
          <span>少</span><i
            v-for="level in 5"
            :key="level"
            class="activity-cell"
            :data-level="level - 1"
          /><span>多</span>
        </div>
      </footer>
    </template>
    <Teleport to="body">
      <div
        v-if="tooltip"
        :id="tooltipId"
        role="tooltip"
        class="token-activity-tooltip"
        :style="{ left: `${tooltip.x}px`, top: `${tooltip.y}px`, transform: tooltip.below ? 'translateX(-50%)' : 'translate(-50%, -100%)' }"
      >
        <div>{{ pointDate(tooltip.index) }}</div><div>{{ mode === 'cumulative' ? '累计 ' : '' }}{{ number(pointValue(tooltip.index)) }} Token</div>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, useId, watch } from 'vue'
import { RefreshCw } from 'lucide-vue-next'
import { meApi } from '@/api/me'
import { buildTokenHistory, tokenLevel, type ActivityMode, type TokenHistory } from './tokenActivity'

const modes: { value: ActivityMode; label: string }[] = [{ value: 'daily', label: '每天' }, { value: 'weekly', label: '每周' }, { value: 'cumulative', label: '累计总量' }]
const mode = ref<ActivityMode>('daily'), history = ref<TokenHistory>(), loading = ref(false), error = ref(false)
const chart = ref<HTMLElement>(), scroller = ref<HTMLElement>(), focused = ref(0)
const tooltipId = useId()
const tooltip = ref<{ index: number; x: number; y: number; below: boolean }>()
let alive = true, version = 0
const number = (value: number) => new Intl.NumberFormat('zh-CN').format(value)
const points = computed(() => mode.value === 'weekly' ? history.value?.weeks ?? [] : history.value?.days ?? [])
const maximum = computed(() => Math.max(0, ...points.value.map((_, index) => pointValue(index))))
const padding = computed(() => history.value?.weeks[0]?.days.findIndex(day => day !== null) ?? 0)
const dayOffset = (week: number, day: number) => week * 7 + day - padding.value
function pointValue(index: number): number {
  if (mode.value === 'weekly') return history.value?.weeks[index]?.tokens ?? 0
  const day = history.value?.days[index]
  return (mode.value === 'cumulative' ? day?.cumulative : day?.tokens) ?? 0
}
function pointDate(index: number): string {
  if (mode.value === 'weekly') { const week = history.value?.weeks[index]; return week ? `${week.start} 至 ${week.end}` : '' }
  return history.value?.days[index]?.date ?? ''
}
const pointLabel = (index: number) => `${pointDate(index)}，${mode.value === 'cumulative' ? '累计 ' : ''}${number(pointValue(index))} Token，UTC`
function hide() { tooltip.value = undefined }
function placeTooltip(index: number, rect: ReturnType<HTMLElement['getBoundingClientRect']>) {
  const below = rect.top < 80
  tooltip.value = { index, x: Math.min(Math.max(rect.left + rect.width / 2, 128), Math.max(128, window.innerWidth - 128)), y: below ? rect.bottom + 8 : rect.top - 8, below }
}
function show(index: number, event: Event) {
  if (!history.value) return
  focused.value = index
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect()
  placeTooltip(index, rect)
}
function handleViewportChange() {
  const cell = document.activeElement
  if (!tooltip.value || !(cell instanceof HTMLButtonElement) || !chart.value?.contains(cell) || Number(cell.dataset.point) !== tooltip.value.index) { hide(); return }
  const rect = cell.getBoundingClientRect()
  if (rect.bottom <= 0 || rect.top >= window.innerHeight || rect.right <= 0 || rect.left >= window.innerWidth) { hide(); return }
  // Native focus may scroll the page after the focus event; keep the tooltip anchored.
  placeTooltip(tooltip.value.index, rect)
}
function navigate(event: KeyboardEvent, index: number) {
  if (event.key === 'Escape') { hide(); return }
  const step = mode.value === 'weekly' ? 1 : 7
  const moves: Record<string, number> = { ArrowLeft: index - step, ArrowRight: index + step, ArrowUp: index - 1, ArrowDown: index + 1, Home: 0, End: points.value.length - 1 }
  const target = moves[event.key]
  if (target === undefined) return
  event.preventDefault()
  focused.value = Math.max(0, Math.min(points.value.length - 1, target))
  chart.value?.querySelector<HTMLButtonElement>(`[data-point="${focused.value}"]`)?.focus()
}
async function resetView() { hide(); focused.value = Math.max(0, points.value.length - 1); await nextTick(); if (alive && scroller.value) scroller.value.scrollLeft = scroller.value.scrollWidth }
async function load(fresh = false) {
  const current = ++version
  loading.value = true; error.value = false; hide()
  try {
    const data = buildTokenHistory(await meApi.getActivityHeatmap(fresh))
    if (alive && current === version) { history.value = data; await resetView() }
  } catch { if (alive && current === version) error.value = true }
  finally { if (alive && current === version) loading.value = false }
}
watch(mode, resetView)
onMounted(() => { void load(); window.addEventListener('scroll', handleViewportChange, true); window.addEventListener('resize', handleViewportChange) })
onUnmounted(() => { alive = false; version++; window.removeEventListener('scroll', handleViewportChange, true); window.removeEventListener('resize', handleViewportChange) })
</script>

<style scoped>
.token-activity { min-width: 0; color: var(--foreground); }
.activity-header { display: flex; justify-content: space-between; align-items: center; gap: .75rem; margin-bottom: 1rem; }
.activity-header h2 { font-size: 14px; font-weight: 500; white-space: nowrap; }
.activity-controls,.activity-modes { display: flex; align-items: center; gap: .25rem; }
.activity-modes button { font-size: 12px; color: var(--muted-foreground); padding: .3rem .35rem; white-space: nowrap; border-radius: 4px; }
.activity-modes button[aria-pressed="true"] { color: var(--foreground); font-weight: 500; }
.activity-modes button:hover,.activity-refresh:hover { background: var(--muted); }
.activity-refresh { height: 28px; width: 28px; display: grid; place-items: center; border-radius: 4px; color: var(--muted-foreground); flex-shrink: 0; }.activity-refresh:disabled { opacity: .5; }
.activity-scroll { overflow-x: auto; padding: 3px 2px 8px; }
.activity-chart { min-width: 640px; }
.activity-columns,.activity-months { display: grid; grid-template-columns: repeat(var(--weeks),minmax(0,1fr)); gap: 3px; }
.activity-week { display: grid; gap: 3px; grid-template-rows: repeat(7,1fr); min-width: 0; }
.activity-cell,.activity-padding { display: block; width: 100%; aspect-ratio: 1; border-radius: 3px; }
button.activity-cell { border: 0; padding: 0; cursor: pointer; }.activity-cell:focus-visible { outline: 2px solid var(--foreground); outline-offset: 2px; }.activity-cell:hover { box-shadow: 0 0 0 1px var(--foreground); }
.activity-cell[data-level="0"] { background: var(--muted); }.activity-cell[data-level="1"] { background: #c9e4ff; }.activity-cell[data-level="2"] { background: #8bc5ff; }.activity-cell[data-level="3"] { background: #3e9bf5; }.activity-cell[data-level="4"] { background: #0969c3; }
.weekly-columns .activity-week { display: flex; align-items: flex-end; aspect-ratio: 1 / 9; }.weekly-bar { aspect-ratio: auto; min-height: 4px; }
.activity-months { margin-top: .5rem; font-size: 11px; line-height: 18px; color: var(--muted-foreground); }.activity-months span { white-space: nowrap; overflow: visible; }
.activity-footer { display: flex; flex-wrap: wrap; justify-content: space-between; gap: .6rem; margin-top: .4rem; font-size: 11px; line-height: 1.7; color: var(--muted-foreground); }.activity-legend { display: flex; align-items: center; gap: 3px; align-self: flex-start; }.activity-legend .activity-cell { width: 9px; height: 9px; flex-shrink: 0; }
.activity-message { font-size: 13px; min-height: 110px; padding: 1.5rem 0; color: var(--muted-foreground); }.activity-message button { text-decoration: underline; margin-left: .5rem; }
.token-activity-tooltip { position: fixed; z-index: 70; pointer-events: none; max-width: 244px; padding: .5rem .75rem; border-radius: 7px; background: #202124; color: #fff; font-size: 12px; line-height: 1.5; font-variant-numeric: tabular-nums; box-shadow: 0 4px 14px #0002; }
:global(.dark) .activity-cell[data-level="1"] { background: #153b60; }:global(.dark) .activity-cell[data-level="2"] { background: #216498; }:global(.dark) .activity-cell[data-level="3"] { background: #3e9bf5; }:global(.dark) .activity-cell[data-level="4"] { background: #9ed1ff; }
@media(max-width: 420px) { .activity-header { gap: .25rem; }.activity-modes { gap: 0; }.activity-modes button { padding: .3rem; }.activity-refresh { width: 24px; } }
</style>
