<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { AlertCircle, CheckCircle2, CircleDashed, Copy, CopyPlus, FlaskConical, Grid3x3, ListChecks, Pencil, Play, Plus, RefreshCw, Save, X, XCircle } from 'lucide-vue-next'
import { PageContainer, PageHeader } from '@/components/layout'
import { Button, Input } from '@/components/ui'
import { compatibilityApi, compatibilityStatus, runSummary, type CompatibilityConfig, type CompatibilitySnapshot, type CompatibilityProfile, type CompatibilityRun } from '@/api/compatibility'
import { getProvidersSummary } from '@/api/endpoints/providers'
import { getProviderModels } from '@/api/endpoints/models'
import { getProviderEndpoints } from '@/api/endpoints/endpoints'
import type { Model, ProviderEndpoint, ProviderWithEndpointsSummary } from '@/api/endpoints/types'
import {
  caseCounts, caseLabel, caseShortLabel, caseStatus, caseTone, EXECUTED_CASES, kpis, overviewProfiles, relativeTime,
  sortByAttention, stateLabel, stateTone, toneDot, tonePill, toneText, type ProfileOverview,
} from './compatibility/compatibility-view'

const data = ref<CompatibilitySnapshot | null>(null)
const busy = ref(false), error = ref(''), editing = ref(false), editId = ref('')
const clientFilter = ref(''), modelFilter = ref(''), statusFilter = ref(''), freshness = ref('all')
const selectedRun = ref<CompatibilityRun | null>(null)
const tab = ref<'profiles' | 'matrix' | 'runs'>('profiles')
const runAllProgress = ref<{ done: number, total: number } | null>(null)
const copied = ref('')
const selectClass = 'h-9 rounded-md border bg-background px-2.5 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-60'
const selectedStale = computed(() => data.value?.runs.find(item => item.run.id === selectedRun.value?.id)?.stale)
const clients = [{ value: 'codex', label: 'Codex' }, { value: 'claude_code', label: 'Claude Code' }, { value: 'open_code', label: 'OpenCode' }, { value: 'generic', label: '通用' }]
const protocols = ['openai:responses', 'openai:chat', 'claude:messages'] as const
function empty(): CompatibilityConfig { return { client: 'codex', client_version: 'unspecified', client_protocol: 'openai:responses', model: 'fixture-model', upstream_format: 'claude:messages', mapped_model: 'fixture-upstream' } }
const draft = ref<CompatibilityConfig>(empty())
const selectionMode = ref('channel')
const providers = ref<ProviderWithEndpointsSummary[]>([])
const channelModels = ref<Model[]>([])
const channelEndpoints = ref<ProviderEndpoint[]>([])
const providerId = ref(''), providerModelId = ref(''), endpointId = ref('')
const selectionBusy = ref(false), selectionError = ref('')
let selectionRequest = 0, disposed = false
const compatibleEndpoints = computed(() => channelEndpoints.value.filter(e => protocols.includes(e.api_format as typeof protocols[number])))
const selectionValid = computed(() => selectionMode.value === 'manual' || !!draft.value.channel)
const controller = new AbortController()
const writable = computed(() => data.value?.can_run === true && !busy.value)
const overview = computed(() => overviewProfiles(data.value))
const totals = computed(() => kpis(overview.value))
const visibleOverview = computed(() => sortByAttention(overview.value.filter(item => matches(item.profile.config)
  && (!statusFilter.value || item.state === statusFilter.value)
  && (freshness.value === 'all' || (freshness.value === 'current' ? !item.needsRun : item.needsRun)))))
const profiles = computed(() => visibleOverview.value.map(item => item.profile))
const runs = computed(() => (data.value?.runs ?? []).filter(r => matches(r.run.config)
  && (!statusFilter.value || runSummary(r.run.cases) === statusFilter.value)
  && (freshness.value === 'all' || (freshness.value === 'current' ? r.stale === false : r.stale === true))))
function matches(config: CompatibilityConfig) { return (!clientFilter.value || config.client === clientFilter.value) && (!modelFilter.value || config.model.toLowerCase().includes(modelFilter.value.toLowerCase())) }
function clientLabel(client: string) { return clients.find(c => c.value === client)?.label ?? '未知客户端' }
function detail(e: unknown) {
  const response = (e as { response?: { status?: number; data?: { detail?: string } } })?.response
  if (response?.status === 409) return '配置已变动，请刷新后重试。'
  return response?.data?.detail ?? (e instanceof Error ? e.message : '工作台请求失败')
}
async function load() {
  if (busy.value) return
  busy.value = true; error.value = ''; selectedRun.value = null; editing.value = false
  try { data.value = await compatibilityApi.list(controller.signal) }
  catch (e) { data.value = null; error.value = detail(e) }
  finally { busy.value = false }
}
function edit(profile?: CompatibilityProfile) {
  editId.value = profile?.id ?? crypto.randomUUID()
  draft.value = profile ? { ...profile.config } : empty()
  editing.value = true
  selectionMode.value = profile && !profile.config.channel ? 'manual' : 'channel'
  providerId.value = profile?.config.channel?.provider_id ?? ''
  providerModelId.value = profile?.config.channel?.provider_model_id ?? ''
  endpointId.value = profile?.config.channel?.endpoint_id ?? ''
  if (selectionMode.value === 'channel') void loadChannels()
}

async function loadChannels() {
  const request = ++selectionRequest
  selectionBusy.value = true; selectionError.value = ''
  try {
    if (!providers.value.length) {
      const items: ProviderWithEndpointsSummary[] = []
      for (let page = 1; page <= 100; page++) {
        const result = await getProvidersSummary({ page, page_size: 100 }, { cacheTtlMs: 30_000 })
        items.push(...result.items)
        if (items.length >= result.total || !result.items.length) break
      }
      if (disposed || request !== selectionRequest) return
      providers.value = items
    }
    if (providerId.value) await loadChannelModels(request)
  } catch (e) { if (!disposed && request === selectionRequest) selectionError.value = detail(e) }
  finally { if (!disposed && request === selectionRequest) selectionBusy.value = false }
}

async function loadChannelModels(request: number) {
  const id = providerId.value
  const models: Model[] = []
  for (let skip = 0; skip < 10_000; skip += 500) {
    const page = await getProviderModels(id, { skip, limit: 500 })
    if (disposed || request !== selectionRequest) return
    models.push(...page)
    if (page.length < 500) break
  }
  const endpoints = await getProviderEndpoints(id)
  if (disposed || request !== selectionRequest) return
  channelModels.value = models
  channelEndpoints.value = endpoints
  if (!endpointId.value) endpointId.value = compatibleEndpoints.value.find(e => e.is_active)?.id ?? compatibleEndpoints.value[0]?.id ?? ''
  applyChannelSelection()
}

function changeProvider() {
  channelModels.value = []; channelEndpoints.value = []
  providerModelId.value = ''; endpointId.value = ''; delete draft.value.channel
  void loadChannels()
}
function changeSelectionMode() {
  if (selectionMode.value === 'manual') delete draft.value.channel
  else { applyChannelSelection(); void loadChannels() }
}
function applyChannelSelection() {
  delete draft.value.channel
  if (selectionMode.value !== 'channel') return
  const model = channelModels.value.find(m => m.id === providerModelId.value)
  const endpoint = compatibleEndpoints.value.find(e => e.id === endpointId.value)
  if (!model || !endpoint) return
  draft.value.model = model.global_model_name || model.provider_model_name
  draft.value.mapped_model = model.provider_model_name
  draft.value.upstream_format = endpoint.api_format as CompatibilityConfig['upstream_format']
  draft.value.channel = { provider_id: providerId.value, provider_model_id: model.id, endpoint_id: endpoint.id }
}
function clientChanged() {
  if (draft.value.client === 'codex') draft.value.client_protocol = 'openai:responses'
  if (draft.value.client === 'claude_code') draft.value.client_protocol = 'claude:messages'
}
async function save() {
  if (!writable.value || !data.value || !selectionValid.value || (selectionMode.value === 'channel' && selectionBusy.value)) return
  busy.value = true; error.value = ''
  try { data.value = await compatibilityApi.saveProfile(data.value.revision, { id: editId.value, config: { ...draft.value } }); editing.value = false }
  catch (e) { error.value = detail(e); data.value = null }
  finally { busy.value = false }
}
async function run(profile: CompatibilityProfile) {
  if (!writable.value || !data.value) return
  busy.value = true; error.value = ''
  try { data.value = await compatibilityApi.run(data.value.revision, profile); selectedRun.value = data.value.runs[0]?.run ?? null }
  catch (e) { error.value = detail(e); data.value = null; selectedRun.value = null }
  finally { busy.value = false }
}
function date(seconds: number) { return Number.isFinite(seconds) ? new Date(seconds * 1000).toLocaleString() : '未知时间' }
function duplicate(profile: CompatibilityProfile) {
  edit({ ...profile, id: crypto.randomUUID() })
}
async function runAll() {
  if (!writable.value || !data.value || !visibleOverview.value.length) return
  const queue = visibleOverview.value.map(item => item.profile.id)
  runAllProgress.value = { done: 0, total: queue.length }
  try {
    for (const id of queue) {
      // Each run returns the new revision, so always run against the latest snapshot.
      const profile = data.value?.profiles.find(item => item.id === id)
      if (!profile || !data.value) break
      busy.value = true
      data.value = await compatibilityApi.run(data.value.revision, profile)
      runAllProgress.value = { done: runAllProgress.value.done + 1, total: queue.length }
    }
  } catch (e) { error.value = detail(e); data.value = null; selectedRun.value = null }
  finally { busy.value = false; runAllProgress.value = null }
}
async function copy(text: string) {
  try { await navigator.clipboard.writeText(text); copied.value = text; setTimeout(() => { if (copied.value === text) copied.value = '' }, 1500) } catch { /* clipboard unavailable */ }
}
function openRun(item: ProfileOverview | { run: CompatibilityRun }) {
  const run = 'run' in item ? item.run : item.latest?.run
  if (run) selectedRun.value = run
}
function closePanels() { if (selectedRun.value) selectedRun.value = null; else editing.value = false }
function onKeydown(event: KeyboardEvent) { if (event.key === 'Escape' && (selectedRun.value || editing.value)) closePanels() }
const filteredRuns = computed(() => runs.value)
onMounted(() => { window.addEventListener('keydown', onKeydown); void load() })
onUnmounted(() => { window.removeEventListener('keydown', onKeydown); disposed = true; selectionRequest++; controller.abort() })
</script>

<template>
  <PageContainer>
    <div class="min-w-0 space-y-5">
      <PageHeader
        title="模型兼容性"
        description="用离线合成用例验证客户端协议到上游格式的转换；结果不代表真实模型或真实客户端的表现。"
        :icon="FlaskConical"
      >
        <template #actions>
          <Button
            variant="outline"
            size="icon"
            title="刷新"
            :disabled="busy"
            @click="load"
          >
            <RefreshCw
              class="h-4 w-4"
              :class="{ 'animate-spin': busy && !runAllProgress }"
            />
          </Button>
          <Button
            variant="outline"
            :disabled="!writable && !runAllProgress || !visibleOverview.length"
            title="按当前筛选依次运行全部配置的合成验收"
            @click="runAll"
          >
            <Play class="mr-2 h-4 w-4" />{{ runAllProgress ? `运行中 ${runAllProgress.done}/${runAllProgress.total}` : '运行全部' }}
          </Button>
          <Button
            :disabled="!writable"
            @click="edit()"
          >
            <Plus class="mr-2 h-4 w-4" />新建配置
          </Button>
        </template>
      </PageHeader>

      <p
        v-if="error"
        role="alert"
        class="flex items-start gap-2 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-600 dark:text-red-400"
      >
        <AlertCircle class="mt-0.5 h-4 w-4 shrink-0" /><span class="min-w-0 break-words">{{ error }}</span>
      </p>

      <section
        class="flex flex-wrap items-center gap-x-5 gap-y-2 rounded-xl border bg-card px-4 py-3 text-xs text-muted-foreground"
        aria-label="证据范围"
      >
        <span class="inline-flex items-center gap-1.5 rounded-full bg-primary/10 px-2.5 py-1 font-medium text-primary">证据：离线合成</span>
        <span class="inline-flex items-center gap-1.5"><span class="h-1.5 w-1.5 rounded-full bg-muted-foreground/40" />真实模型：未测试</span>
        <span class="inline-flex items-center gap-1.5"><span class="h-1.5 w-1.5 rounded-full bg-muted-foreground/40" />真实客户端：未测试</span>
        <span>指纹范围：工作台配置与转换库源码</span>
        <span class="ml-auto font-mono">Revision {{ data?.revision ?? '—' }}</span>
      </section>

      <section
        class="grid grid-cols-2 gap-3 lg:grid-cols-4"
        aria-label="概览"
      >
        <div class="rounded-xl border bg-card p-4">
          <p class="text-xs text-muted-foreground">
            配置
          </p>
          <p class="mt-1 text-2xl font-semibold tabular-nums">
            {{ totals.total }}<span class="text-sm font-normal text-muted-foreground"> / 64</span>
          </p>
        </div>
        <div class="rounded-xl border bg-card p-4">
          <p class="text-xs text-muted-foreground">
            合成通过（当前指纹）
          </p>
          <p
            class="mt-1 text-2xl font-semibold tabular-nums"
            :class="totals.passed ? toneText('success') : ''"
          >
            {{ totals.passed }}
          </p>
        </div>
        <div class="rounded-xl border bg-card p-4">
          <p class="text-xs text-muted-foreground">
            合成失败
          </p>
          <p
            class="mt-1 text-2xl font-semibold tabular-nums"
            :class="totals.failed ? toneText('danger') : ''"
          >
            {{ totals.failed }}
          </p>
        </div>
        <div class="rounded-xl border bg-card p-4">
          <p class="text-xs text-muted-foreground">
            待运行 / 已过期
          </p>
          <p
            class="mt-1 text-2xl font-semibold tabular-nums"
            :class="totals.needsRun ? toneText('warn') : ''"
          >
            {{ totals.needsRun }}
          </p>
        </div>
      </section>

      <div class="flex flex-wrap items-end justify-between gap-3">
        <div
          class="inline-flex rounded-lg border bg-muted/40 p-1"
          role="tablist"
          aria-label="视图"
        >
          <button
            v-for="item in [{ id: 'profiles', label: '配置', icon: ListChecks, count: profiles.length }, { id: 'matrix', label: '覆盖矩阵', icon: Grid3x3, count: undefined }, { id: 'runs', label: '合成记录', icon: CircleDashed, count: filteredRuns.length }] as const"
            :key="item.id"
            type="button"
            role="tab"
            :aria-selected="tab === item.id"
            class="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm transition-colors"
            :class="tab === item.id ? 'bg-background font-medium shadow-sm' : 'text-muted-foreground hover:text-foreground'"
            @click="tab = item.id"
          >
            <component
              :is="item.icon"
              class="h-3.5 w-3.5"
            />{{ item.label }}<span
              v-if="item.count !== undefined"
              class="rounded-full bg-muted px-1.5 text-xs tabular-nums text-muted-foreground"
            >{{ item.count }}</span>
          </button>
        </div>
        <div class="flex flex-wrap items-end gap-3">
          <label class="space-y-1 text-xs text-muted-foreground">客户端<select
            v-model="clientFilter"
            aria-label="筛选客户端"
            class="block"
            :class="[selectClass]"
          ><option value="">全部客户端</option><option
            v-for="client in clients"
            :key="client.value"
            :value="client.value"
          >{{ client.label }}</option></select></label>
          <label class="space-y-1 text-xs text-muted-foreground">模型<Input
            v-model="modelFilter"
            aria-label="筛选模型"
            placeholder="搜索模型"
            class="h-9 w-48"
          /></label>
          <label class="space-y-1 text-xs text-muted-foreground">合成结果<select
            v-model="statusFilter"
            aria-label="筛选结果"
            class="block"
            :class="[selectClass]"
          ><option value="">全部结果</option><option
            v-for="status in ['passed', 'failed', 'not_tested', 'unsupported', 'unknown', 'none']"
            :key="status"
            :value="status"
          >{{ stateLabel(status as never) }}</option></select></label>
          <label class="space-y-1 text-xs text-muted-foreground">配置状态<select
            v-model="freshness"
            aria-label="配置状态"
            class="block"
            :class="[selectClass]"
          ><option value="all">全部记录</option><option value="current">当前指纹</option><option value="stale">已过期</option></select></label>
        </div>
      </div>

      <!-- 配置 -->
      <section
        v-if="tab === 'profiles'"
        class="min-w-0"
        aria-label="配置列表"
      >
        <div
          v-if="visibleOverview.length"
          class="grid gap-4 md:grid-cols-2 2xl:grid-cols-3"
        >
          <article
            v-for="item in visibleOverview"
            :key="item.profile.id"
            class="group flex min-w-0 flex-col rounded-xl border bg-card p-4 shadow-sm transition-shadow hover:shadow-md"
            :class="item.state === 'failed' ? 'border-red-500/30' : ''"
          >
            <header class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <p class="text-xs text-muted-foreground">
                  {{ clientLabel(item.profile.config.client) }} · {{ item.profile.config.client_version }}
                </p>
                <h3 class="mt-0.5 break-all text-base font-semibold leading-snug">
                  {{ item.profile.config.model }}
                </h3>
                <p class="mt-0.5 break-all text-xs text-muted-foreground">
                  → {{ item.profile.config.mapped_model }}
                </p>
              </div>
              <span
                class="inline-flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full px-2.5 py-1 text-xs font-medium ring-1 ring-inset"
                :class="tonePill(stateTone(item.state))"
              >
                <CheckCircle2
                  v-if="item.state === 'passed'"
                  class="h-3.5 w-3.5"
                /><XCircle
                  v-else-if="item.state === 'failed'"
                  class="h-3.5 w-3.5"
                /><CircleDashed
                  v-else
                  class="h-3.5 w-3.5"
                />{{ stateLabel(item.state) }}
              </span>
            </header>

            <div class="mt-3 flex flex-wrap items-center gap-1.5 font-mono text-[11px]">
              <span class="rounded-md bg-muted px-2 py-0.5">{{ item.profile.config.client_protocol }}</span>
              <span class="text-muted-foreground">→</span>
              <span class="rounded-md bg-muted px-2 py-0.5">{{ item.profile.config.upstream_format }}</span>
              <span
                v-if="item.profile.config.channel"
                class="rounded-md bg-primary/10 px-2 py-0.5 font-sans text-primary"
              >渠道模型</span>
            </div>

            <div class="mt-4 space-y-1.5">
              <div
                class="flex gap-1"
                role="img"
                :aria-label="item.latest ? '最近一次各场景结果' : '尚无合成记录'"
              >
                <span
                  v-for="name in EXECUTED_CASES"
                  :key="name"
                  class="h-1.5 flex-1 rounded-full"
                  :class="toneDot(caseTone(caseStatus(item.latest?.run, name)))"
                  :title="`${caseLabel(name)}：${compatibilityStatus(caseStatus(item.latest?.run, name) ?? 'not_tested')}`"
                />
              </div>
              <p class="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
                <template v-if="item.latest">
                  <span>最近合成 {{ relativeTime(item.latest.run.created_at) }}</span>
                  <span :class="item.needsRun ? toneText('warn') : ''">{{ item.needsRun ? '已过期' : '当前指纹' }}</span>
                </template>
                <span v-else>尚未运行合成验收</span>
                <button
                  type="button"
                  class="ml-auto inline-flex items-center gap-1 font-mono hover:text-foreground"
                  :title="`复制指纹 ${item.profile.config_fingerprint}`"
                  @click="copy(item.profile.config_fingerprint)"
                >
                  <Copy class="h-3 w-3" />{{ copied === item.profile.config_fingerprint ? '已复制' : item.profile.config_fingerprint.slice(0, 8) }}
                </button>
              </p>
            </div>

            <footer class="mt-4 flex flex-wrap items-center gap-2 border-t pt-3">
              <Button
                size="sm"
                :variant="item.needsRun ? 'default' : 'outline'"
                :disabled="!writable"
                title="仅运行离线合成协议转换"
                @click="run(item.profile)"
              >
                <Play class="mr-1.5 h-3.5 w-3.5" />合成验收
              </Button>
              <Button
                v-if="item.latest"
                size="sm"
                variant="ghost"
                @click="openRun(item)"
              >
                最近结果
              </Button>
              <span class="ml-auto flex items-center">
                <Button
                  variant="ghost"
                  size="icon"
                  title="复制为新配置"
                  :disabled="!writable"
                  @click="duplicate(item.profile)"
                ><CopyPlus class="h-4 w-4" /></Button>
                <Button
                  variant="ghost"
                  size="icon"
                  title="编辑配置"
                  :disabled="!writable"
                  @click="edit(item.profile)"
                ><Pencil class="h-4 w-4" /></Button>
              </span>
            </footer>
          </article>
        </div>
        <div
          v-else
          class="flex flex-col items-center gap-3 rounded-xl border border-dashed py-14 text-center"
        >
          <FlaskConical class="h-8 w-8 text-muted-foreground/50" />
          <p class="text-sm text-muted-foreground">
            {{ busy ? '加载中…' : data ? (overview.length ? '无匹配配置' : '还没有兼容性配置') : '未连接' }}
          </p>
          <Button
            v-if="data && !overview.length"
            :disabled="!writable"
            @click="edit()"
          >
            <Plus class="mr-2 h-4 w-4" />新建第一个配置
          </Button>
        </div>
      </section>

      <!-- 覆盖矩阵 -->
      <section
        v-else-if="tab === 'matrix'"
        class="min-w-0 space-y-2"
        aria-label="覆盖矩阵"
      >
        <p class="text-xs text-muted-foreground">
          每个配置最近一次合成记录的逐场景结果；点击单元格查看详情。过期记录以虚线标出。
        </p>
        <div class="overflow-x-auto rounded-xl border bg-card">
          <table class="w-full min-w-[860px] text-left text-xs">
            <thead class="border-b bg-muted/40 text-muted-foreground">
              <tr>
                <th class="p-3 font-medium">
                  配置
                </th>
                <th
                  v-for="name in EXECUTED_CASES"
                  :key="name"
                  class="p-3 text-center font-medium"
                  :title="caseLabel(name)"
                >
                  {{ caseShortLabel(name) }}
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="item in visibleOverview"
                :key="item.profile.id"
                class="border-b last:border-0"
              >
                <td class="max-w-64 p-3">
                  <p class="break-all font-medium">
                    {{ item.profile.config.model }}
                  </p>
                  <p class="text-muted-foreground">
                    {{ clientLabel(item.profile.config.client) }} · {{ item.profile.config.client_version }}
                  </p>
                </td>
                <td
                  v-for="name in EXECUTED_CASES"
                  :key="name"
                  class="p-2 text-center"
                >
                  <button
                    type="button"
                    class="inline-flex h-7 w-9 items-center justify-center rounded-md ring-1 ring-inset transition-transform hover:scale-105 disabled:cursor-default"
                    :class="[tonePill(caseTone(caseStatus(item.latest?.run, name))), item.needsRun && item.latest ? 'opacity-60 [border:1px_dashed_currentColor]' : '']"
                    :disabled="!item.latest"
                    :title="`${caseLabel(name)}：${compatibilityStatus(caseStatus(item.latest?.run, name) ?? 'not_tested')}${item.needsRun && item.latest ? '（已过期）' : ''}`"
                    @click="openRun(item)"
                  >
                    <CheckCircle2
                      v-if="caseStatus(item.latest?.run, name) === 'passed'"
                      class="h-3.5 w-3.5"
                    /><XCircle
                      v-else-if="caseStatus(item.latest?.run, name) === 'failed'"
                      class="h-3.5 w-3.5"
                    /><span v-else>–</span>
                  </button>
                </td>
              </tr>
              <tr v-if="!visibleOverview.length">
                <td
                  :colspan="EXECUTED_CASES.length + 1"
                  class="p-8 text-center text-muted-foreground"
                >
                  {{ busy ? '加载中…' : data ? '无匹配配置' : '未连接' }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <!-- 合成记录 -->
      <section
        v-else
        class="min-w-0 space-y-2"
        aria-label="合成记录"
      >
        <p class="text-xs text-muted-foreground">
          最多保留 200 条合成记录。
        </p>
        <div class="overflow-x-auto rounded-xl border bg-card">
          <table class="w-full min-w-[860px] text-left text-xs">
            <thead class="border-b bg-muted/40 text-muted-foreground">
              <tr>
                <th class="p-3 font-medium">
                  时间
                </th><th class="p-3 font-medium">
                  客户端 / 版本
                </th><th class="p-3 font-medium">
                  模型 / 协议
                </th><th class="p-3 font-medium">
                  配置
                </th><th class="p-3 font-medium">
                  合成结果
                </th><th class="p-3 font-medium">
                  真实模型 / 客户端
                </th><th class="p-3" />
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="item in filteredRuns"
                :key="item.run.id"
                class="border-b last:border-0"
                :class="{ 'bg-muted/40': selectedRun?.id === item.run.id }"
              >
                <td class="whitespace-nowrap p-3">
                  <p>{{ relativeTime(item.run.created_at) }}</p>
                  <p class="text-muted-foreground">
                    {{ date(item.run.created_at) }}
                  </p>
                </td>
                <td class="max-w-40 break-all p-3">
                  {{ clientLabel(item.run.config.client) }}<div class="text-muted-foreground">
                    {{ item.run.config.client_version }}
                  </div>
                </td>
                <td class="max-w-56 break-all p-3">
                  {{ item.run.config.model }}<div class="font-mono text-muted-foreground">
                    {{ item.run.config.client_protocol }} → {{ item.run.config.upstream_format }}
                  </div>
                </td>
                <td class="p-3">
                  <span :class="item.stale !== false ? toneText('warn') : 'text-muted-foreground'">{{ item.stale === true ? '已过期' : item.stale === false ? '当前指纹' : '未知（非当前）' }}</span><div
                    class="font-mono text-muted-foreground"
                    :title="item.run.config_fingerprint"
                  >
                    {{ item.run.config_fingerprint.slice(0, 12) }}
                  </div>
                </td>
                <td class="p-3">
                  <span
                    class="inline-flex items-center rounded-full px-2.5 py-0.5 font-medium ring-1 ring-inset"
                    :class="tonePill(item.run.evidence === 'offline_synthetic' ? stateTone(runSummary(item.run.cases) as never) : 'warn')"
                  >{{ item.run.evidence === 'offline_synthetic' ? compatibilityStatus(runSummary(item.run.cases)) : '未知证据（非通过）' }}</span>
                  <div class="mt-1.5 flex w-28 gap-0.5">
                    <span
                      v-for="name in EXECUTED_CASES"
                      :key="name"
                      class="h-1 flex-1 rounded-full"
                      :class="toneDot(caseTone(caseStatus(item.run, name)))"
                    />
                  </div>
                </td>
                <td class="p-3 text-muted-foreground">
                  未测 / 未测
                </td>
                <td class="p-3 text-right">
                  <Button
                    variant="ghost"
                    size="sm"
                    @click="selectedRun = item.run"
                  >
                    逐项结果
                  </Button>
                </td>
              </tr>
              <tr v-if="!filteredRuns.length">
                <td
                  colspan="7"
                  class="p-8 text-center text-muted-foreground"
                >
                  {{ busy ? '加载中…' : '暂无匹配记录' }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <!-- 编辑 / 新建 -->
      <div
        v-if="editing"
        class="fixed inset-0 z-40 flex justify-end bg-black/40"
        @click.self="editing = false"
      >
        <form
          class="flex h-full w-full max-w-xl flex-col border-l bg-background shadow-2xl"
          aria-label="配置表单"
          @submit.prevent="save"
        >
          <header class="flex items-center justify-between border-b px-5 py-4">
            <div>
              <h2 class="text-base font-semibold">
                {{ data?.profiles.some(p => p.id === editId) ? '编辑配置' : '新建配置' }}
              </h2>
              <p class="mt-0.5 text-xs text-muted-foreground">
                选择一个真实渠道模型自动填充，或手动指定协议与模型。
              </p>
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              title="关闭编辑"
              @click="editing = false"
            >
              <X class="h-4 w-4" />
            </Button>
          </header>
          <div class="min-h-0 flex-1 space-y-6 overflow-y-auto px-5 py-5">
            <section class="space-y-3">
              <h3 class="text-sm font-medium">
                测试对象
              </h3>
              <label class="block space-y-1 text-xs text-muted-foreground">配置来源<select
                v-model="selectionMode"
                aria-label="配置来源"
                class="block w-full"
                :class="[selectClass]"
                @change="changeSelectionMode"
              >
                <option value="channel">渠道模型</option><option value="manual">手动配置</option>
              </select></label>
              <template v-if="selectionMode === 'channel'">
                <label class="block min-w-0 space-y-1 text-xs text-muted-foreground">渠道<select
                  v-model="providerId"
                  aria-label="测试渠道"
                  :disabled="selectionBusy"
                  class="block w-full"
                  :class="[selectClass]"
                  @change="changeProvider"
                >
                  <option value="">选择渠道</option><option
                    v-for="provider in providers"
                    :key="provider.id"
                    :value="provider.id"
                  >{{ provider.name }}{{ provider.is_active ? '' : '（已停用）' }}</option>
                </select></label>
                <label class="block min-w-0 space-y-1 text-xs text-muted-foreground">渠道模型<select
                  v-model="providerModelId"
                  aria-label="测试渠道模型"
                  :disabled="selectionBusy || !providerId"
                  class="block w-full"
                  :class="[selectClass]"
                  @change="applyChannelSelection"
                >
                  <option value="">{{ selectionBusy ? '加载中…' : '选择模型' }}</option><option
                    v-for="model in channelModels"
                    :key="model.id"
                    :value="model.id"
                  >{{ model.global_model_name || model.provider_model_name }} → {{ model.provider_model_name }}</option>
                </select></label>
                <label class="block min-w-0 space-y-1 text-xs text-muted-foreground">渠道端点<select
                  v-model="endpointId"
                  aria-label="测试渠道端点"
                  :disabled="selectionBusy || !providerId"
                  class="block w-full"
                  :class="[selectClass]"
                  @change="applyChannelSelection"
                >
                  <option value="">选择端点</option><option
                    v-for="endpoint in compatibleEndpoints"
                    :key="endpoint.id"
                    :value="endpoint.id"
                  >{{ endpoint.api_format }}{{ endpoint.is_active ? '' : '（已停用）' }}</option>
                </select></label>
              </template>
              <p
                v-if="selectionError"
                role="alert"
                class="text-sm text-destructive"
              >
                {{ selectionError }}
              </p>
            </section>

            <fieldset
              :disabled="!writable"
              class="space-y-3"
            >
              <legend class="mb-3 text-sm font-medium">
                客户端与协议
              </legend>
              <div class="grid gap-3 sm:grid-cols-2">
                <label class="space-y-1 text-xs text-muted-foreground">客户端<select
                  v-model="draft.client"
                  aria-label="配置客户端"
                  class="block w-full"
                  :class="[selectClass]"
                  @change="clientChanged"
                ><option
                  v-for="client in clients"
                  :key="client.value"
                  :value="client.value"
                >{{ client.label }}</option></select></label>
                <label class="space-y-1 text-xs text-muted-foreground">客户端版本<Input
                  v-model="draft.client_version"
                  required
                  maxlength="40"
                  aria-label="客户端版本"
                /></label>
                <label class="space-y-1 text-xs text-muted-foreground">客户端协议<select
                  v-model="draft.client_protocol"
                  aria-label="客户端协议"
                  :disabled="draft.client === 'codex' || draft.client === 'claude_code'"
                  class="block w-full"
                  :class="[selectClass]"
                ><option
                  v-for="protocol in protocols"
                  :key="protocol"
                >{{ protocol }}</option></select></label>
                <label class="space-y-1 text-xs text-muted-foreground">上游格式<select
                  v-model="draft.upstream_format"
                  aria-label="上游格式"
                  :disabled="selectionMode === 'channel'"
                  class="block w-full"
                  :class="[selectClass]"
                ><option
                  v-for="protocol in protocols"
                  :key="protocol"
                >{{ protocol }}</option></select></label>
                <label class="space-y-1 text-xs text-muted-foreground">模型标识<Input
                  v-model="draft.model"
                  required
                  maxlength="128"
                  aria-label="模型标识"
                  :readonly="selectionMode === 'channel'"
                /></label>
                <label class="space-y-1 text-xs text-muted-foreground">映射模型标识<Input
                  v-model="draft.mapped_model"
                  required
                  maxlength="128"
                  aria-label="映射模型标识"
                  :readonly="selectionMode === 'channel'"
                /></label>
              </div>
            </fieldset>

            <p class="rounded-lg bg-muted/50 px-3 py-2 font-mono text-xs">
              {{ draft.client_protocol }} <span class="text-muted-foreground">→</span> {{ draft.upstream_format }}
              <span class="block pt-1 font-sans text-muted-foreground">{{ draft.model || '模型' }} → {{ draft.mapped_model || '映射模型' }}</span>
            </p>
          </div>
          <footer class="flex items-center justify-end gap-2 border-t px-5 py-3">
            <Button
              type="button"
              variant="outline"
              @click="editing = false"
            >
              取消
            </Button>
            <Button
              type="submit"
              :disabled="!writable || !selectionValid || (selectionMode === 'channel' && selectionBusy)"
            >
              <Save class="mr-2 h-4 w-4" />保存配置
            </Button>
          </footer>
        </form>
      </div>

      <!-- 逐项结果 -->
      <div
        v-if="selectedRun"
        class="fixed inset-0 z-50 flex justify-end bg-black/40"
        @click.self="selectedRun = null"
      >
        <section
          class="flex h-full w-full max-w-xl flex-col border-l bg-background shadow-2xl"
          aria-label="逐项结果"
        >
          <header class="flex items-start justify-between gap-3 border-b px-5 py-4">
            <div class="min-w-0">
              <h2 class="text-base font-semibold">
                逐项结果
              </h2>
              <p class="mt-0.5 break-all text-sm">
                {{ selectedRun.config.model }}
                <span class="text-muted-foreground">· {{ clientLabel(selectedRun.config.client) }} {{ selectedRun.config.client_version }}</span>
              </p>
              <p class="mt-0.5 font-mono text-xs text-muted-foreground">
                {{ selectedRun.config.client_protocol }} → {{ selectedRun.config.upstream_format }}
              </p>
            </div>
            <Button
              variant="ghost"
              size="icon"
              title="关闭"
              @click="selectedRun = null"
            >
              <X class="h-4 w-4" />
            </Button>
          </header>
          <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
            <div class="flex flex-wrap items-center gap-2 text-xs">
              <span
                class="rounded-full px-2.5 py-1 font-medium ring-1 ring-inset"
                :class="tonePill(selectedStale === false ? 'success' : 'warn')"
              >{{ selectedStale === true ? '已过期' : selectedStale === false ? '当前指纹' : '未知（非当前）' }}</span>
              <span class="text-muted-foreground">{{ date(selectedRun.created_at) }}</span>
              <button
                type="button"
                class="ml-auto inline-flex items-center gap-1 font-mono text-muted-foreground hover:text-foreground"
                :title="`复制记录 ID ${selectedRun.id}`"
                @click="copy(selectedRun.id)"
              >
                <Copy class="h-3 w-3" />{{ copied === selectedRun.id ? '已复制' : selectedRun.id.slice(0, 8) }}
              </button>
            </div>
            <div class="grid grid-cols-4 gap-2 text-center text-xs">
              <div class="rounded-lg border p-2">
                <p
                  class="text-lg font-semibold tabular-nums"
                  :class="toneText('success')"
                >
                  {{ caseCounts(selectedRun.cases).passed }}
                </p>通过
              </div>
              <div class="rounded-lg border p-2">
                <p
                  class="text-lg font-semibold tabular-nums"
                  :class="caseCounts(selectedRun.cases).failed ? toneText('danger') : ''"
                >
                  {{ caseCounts(selectedRun.cases).failed }}
                </p>失败
              </div>
              <div class="rounded-lg border p-2">
                <p
                  class="text-lg font-semibold tabular-nums"
                  :class="caseCounts(selectedRun.cases).unsupported ? toneText('warn') : ''"
                >
                  {{ caseCounts(selectedRun.cases).unsupported }}
                </p>不支持
              </div>
              <div class="rounded-lg border p-2">
                <p class="text-lg font-semibold tabular-nums">
                  {{ caseCounts(selectedRun.cases).other }}
                </p>其他
              </div>
            </div>
            <ul class="divide-y rounded-xl border">
              <li
                v-for="item in selectedRun.cases"
                :key="item.case"
                class="flex gap-3 p-3 text-sm"
              >
                <span
                  class="mt-1 h-2 w-2 shrink-0 rounded-full"
                  :class="toneDot(caseTone(item.status))"
                />
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <span class="font-medium">{{ caseLabel(item.case) }}</span>
                    <span
                      class="text-xs"
                      :class="toneText(caseTone(item.status))"
                    >{{ compatibilityStatus(item.status) }}</span>
                  </div>
                  <p class="mt-0.5 break-words text-xs text-muted-foreground">
                    {{ item.reason }}
                  </p>
                </div>
              </li>
            </ul>
          </div>
        </section>
      </div>
    </div>
  </PageContainer>
</template>
