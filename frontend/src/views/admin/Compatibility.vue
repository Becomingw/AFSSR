<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { FlaskConical, Play, Plus, RefreshCw, Save, X } from 'lucide-vue-next'
import { PageContainer, PageHeader } from '@/components/layout'
import { Button, Input } from '@/components/ui'
import { compatibilityApi, compatibilityStatus, runSummary, type CompatibilityConfig, type CompatibilitySnapshot, type CompatibilityProfile, type CompatibilityRun } from '@/api/compatibility'
import { getProvidersSummary } from '@/api/endpoints/providers'
import { getProviderModels } from '@/api/endpoints/models'
import { getProviderEndpoints } from '@/api/endpoints/endpoints'
import type { Model, ProviderEndpoint, ProviderWithEndpointsSummary } from '@/api/endpoints/types'

const data = ref<CompatibilitySnapshot | null>(null)
const busy = ref(false), error = ref(''), editing = ref(false), editId = ref('')
const clientFilter = ref(''), modelFilter = ref(''), statusFilter = ref(''), freshness = ref('all')
const selectedRun = ref<CompatibilityRun | null>(null)
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
const profiles = computed(() => (data.value?.profiles ?? []).filter(p => matches(p.config)))
const runs = computed(() => (data.value?.runs ?? []).filter(r => matches(r.run.config)
  && (!statusFilter.value || runSummary(r.run.cases) === statusFilter.value)
  && (freshness.value === 'all' || (freshness.value === 'current' ? r.stale === false : r.stale === true))))
function matches(config: CompatibilityConfig) { return (!clientFilter.value || config.client === clientFilter.value) && (!modelFilter.value || config.model.toLowerCase().includes(modelFilter.value.toLowerCase())) }
function clientLabel(client: string) { return clients.find(c => c.value === client)?.label ?? '未知客户端' }
function caseLabel(value: string) { return ({ basic_request: '基础请求', tool_definition: '工具定义', tool_return: '工具返回', multi_turn_tools: '多轮工具', long_history: '长历史（64 条消息）', image: '图片载体', sync_response: '同步响应', tool_response: '工具调用响应', stream: '文本 SSE', cancel: '取消', compression: '压缩与恢复', real_client: '真实客户端', real_model: '真实模型' } as Record<string, string>)[value] ?? '未知项目' }
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
onMounted(load)
onUnmounted(() => { disposed = true; selectionRequest++; controller.abort() })
</script>

<template>
  <PageContainer>
    <PageHeader
      title="模型兼容性"
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
          <RefreshCw class="h-4 w-4" />
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
      class="break-words text-sm text-destructive"
    >
      {{ error }}
    </p>
    <div class="flex flex-wrap gap-x-6 gap-y-1 border-y py-3 text-xs text-muted-foreground">
      <span>证据：离线合成</span><span>真实模型：未测试</span><span>真实客户端：未测试</span><span>指纹范围：工作台配置与转换库源码</span><span>Revision {{ data?.revision ?? '—' }}</span>
    </div>
    <div class="flex flex-wrap items-end gap-3">
      <label class="space-y-1 text-xs">客户端<select
        v-model="clientFilter"
        aria-label="筛选客户端"
        class="block h-9 rounded-md border bg-background px-2 text-sm"
      ><option value="">全部客户端</option><option
        v-for="client in clients"
        :key="client.value"
        :value="client.value"
      >{{ client.label }}</option></select></label>
      <label class="space-y-1 text-xs">模型<Input
        v-model="modelFilter"
        aria-label="筛选模型"
        class="h-9 w-52"
      /></label>
      <label class="space-y-1 text-xs">合成结果<select
        v-model="statusFilter"
        aria-label="筛选结果"
        class="block h-9 rounded-md border bg-background px-2 text-sm"
      ><option value="">全部结果</option><option
        v-for="status in ['passed', 'failed', 'not_tested', 'unsupported', 'unknown']"
        :key="status"
        :value="status"
      >{{ compatibilityStatus(status) }}</option></select></label>
      <label class="space-y-1 text-xs">配置状态<select
        v-model="freshness"
        aria-label="配置状态"
        class="block h-9 rounded-md border bg-background px-2 text-sm"
      ><option value="all">全部记录</option><option value="current">当前指纹</option><option value="stale">已过期</option></select></label>
    </div>
    <form
      v-if="editing"
      class="space-y-3 border-y py-4"
      @submit.prevent="save"
    >
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold">
          {{ data?.profiles.some(p => p.id === editId) ? '编辑配置' : '新建配置' }}
        </h2><Button
          type="button"
          variant="ghost"
          size="icon"
          title="关闭编辑"
          @click="editing = false"
        >
          <X class="h-4 w-4" />
        </Button>
      </div>
      <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <label class="space-y-1 text-xs">配置来源<select
          v-model="selectionMode"
          aria-label="配置来源"
          class="block h-9 w-full rounded-md border bg-background px-2 text-sm"
          @change="changeSelectionMode"
        >
          <option value="channel">渠道模型</option><option value="manual">手动配置</option>
        </select></label>
        <template v-if="selectionMode === 'channel'">
          <label class="min-w-0 space-y-1 text-xs">渠道<select
            v-model="providerId"
            aria-label="测试渠道"
            :disabled="selectionBusy"
            class="block h-9 w-full rounded-md border bg-background px-2 text-sm"
            @change="changeProvider"
          >
            <option value="">选择渠道</option><option
              v-for="provider in providers"
              :key="provider.id"
              :value="provider.id"
            >{{ provider.name }}{{ provider.is_active ? '' : '（已停用）' }}</option>
          </select></label>
          <label class="min-w-0 space-y-1 text-xs">渠道模型<select
            v-model="providerModelId"
            aria-label="测试渠道模型"
            :disabled="selectionBusy || !providerId"
            class="block h-9 w-full rounded-md border bg-background px-2 text-sm"
            @change="applyChannelSelection"
          >
            <option value="">{{ selectionBusy ? '加载中…' : '选择模型' }}</option><option
              v-for="model in channelModels"
              :key="model.id"
              :value="model.id"
            >{{ model.global_model_name || model.provider_model_name }} → {{ model.provider_model_name }}</option>
          </select></label>
          <label class="min-w-0 space-y-1 text-xs">渠道端点<select
            v-model="endpointId"
            aria-label="测试渠道端点"
            :disabled="selectionBusy || !providerId"
            class="block h-9 w-full rounded-md border bg-background px-2 text-sm"
            @change="applyChannelSelection"
          >
            <option value="">选择端点</option><option
              v-for="endpoint in compatibleEndpoints"
              :key="endpoint.id"
              :value="endpoint.id"
            >{{ endpoint.api_format }}{{ endpoint.is_active ? '' : '（已停用）' }}</option>
          </select></label>
        </template>
      </div>
      <p
        v-if="selectionError"
        role="alert"
        class="text-sm text-destructive"
      >
        {{ selectionError }}
      </p>
      <fieldset
        :disabled="!writable"
        class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3"
      >
        <label class="space-y-1 text-xs">客户端<select
          v-model="draft.client"
          aria-label="配置客户端"
          class="block h-9 w-full rounded-md border bg-background px-2 text-sm"
          @change="clientChanged"
        ><option
          v-for="client in clients"
          :key="client.value"
          :value="client.value"
        >{{ client.label }}</option></select></label>
        <label class="space-y-1 text-xs">客户端版本<Input
          v-model="draft.client_version"
          required
          maxlength="40"
          aria-label="客户端版本"
        /></label>
        <label class="space-y-1 text-xs">客户端协议<select
          v-model="draft.client_protocol"
          aria-label="客户端协议"
          :disabled="draft.client === 'codex' || draft.client === 'claude_code'"
          class="block h-9 w-full rounded-md border bg-background px-2 text-sm"
        ><option
          v-for="protocol in protocols"
          :key="protocol"
        >{{ protocol }}</option></select></label>
        <label class="space-y-1 text-xs">模型标识<Input
          v-model="draft.model"
          required
          maxlength="128"
          aria-label="模型标识"
          :readonly="selectionMode === 'channel'"
        /></label>
        <label class="space-y-1 text-xs">上游格式<select
          v-model="draft.upstream_format"
          aria-label="上游格式"
          :disabled="selectionMode === 'channel'"
          class="block h-9 w-full rounded-md border bg-background px-2 text-sm"
        ><option
          v-for="protocol in protocols"
          :key="protocol"
        >{{ protocol }}</option></select></label>
        <label class="space-y-1 text-xs">映射模型标识<Input
          v-model="draft.mapped_model"
          required
          maxlength="128"
          aria-label="映射模型标识"
          :readonly="selectionMode === 'channel'"
        /></label>
      </fieldset>
      <Button
        type="submit"
        :disabled="!writable || !selectionValid || (selectionMode === 'channel' && selectionBusy)"
      >
        <Save class="mr-2 h-4 w-4" />保存配置
      </Button>
    </form>
    <section class="min-w-0 space-y-2">
      <h2 class="text-sm font-semibold">
        配置（{{ profiles.length }} / 64）
      </h2>
      <div class="overflow-x-auto">
        <table class="w-full min-w-[850px] text-left text-xs">
          <thead class="border-b text-muted-foreground">
            <tr>
              <th class="p-2">
                客户端 / 版本
              </th><th class="p-2">
                模型 / 映射
              </th><th class="p-2">
                客户端协议 → 上游
              </th><th class="p-2">
                当前指纹
              </th><th class="p-2 text-right">
                操作
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="profile in profiles"
              :key="profile.id"
              class="border-b"
            >
              <td class="p-2">
                <span>{{ clientLabel(profile.config.client) }}</span><div class="max-w-40 break-all text-muted-foreground">
                  {{ profile.config.client_version }}
                </div>
              </td>
              <td class="max-w-52 break-all p-2">
                {{ profile.config.model }}<div class="text-muted-foreground">
                  {{ profile.config.mapped_model }}
                </div>
              </td>
              <td class="p-2 font-mono">
                {{ profile.config.client_protocol }}<div class="text-muted-foreground">
                  → {{ profile.config.upstream_format }}
                </div>
              </td>
              <td
                class="p-2 font-mono"
                :title="profile.config_fingerprint"
              >
                {{ profile.config_fingerprint.slice(0, 12) }}
              </td>
              <td class="whitespace-nowrap p-2 text-right">
                <Button
                  variant="ghost"
                  size="sm"
                  :disabled="!writable"
                  @click="edit(profile)"
                >
                  编辑
                </Button><Button
                  variant="outline"
                  size="sm"
                  :disabled="!writable"
                  title="仅运行离线合成协议转换"
                  @click="run(profile)"
                >
                  <Play class="mr-1 h-3 w-3" />合成验收
                </Button>
              </td>
            </tr><tr v-if="!profiles.length">
              <td
                colspan="5"
                class="p-6 text-center text-muted-foreground"
              >
                {{ busy ? '加载中…' : data ? '无匹配配置' : '未连接' }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
    <section class="min-w-0 space-y-2">
      <h2 class="text-sm font-semibold">
        合成记录（{{ runs.length }}，最多保留 200 条）
      </h2>
      <div class="overflow-x-auto">
        <table class="w-full min-w-[850px] text-left text-xs">
          <thead class="border-b text-muted-foreground">
            <tr>
              <th class="p-2">
                时间
              </th><th class="p-2">
                客户端 / 版本
              </th><th class="p-2">
                模型 / 协议
              </th><th class="p-2">
                配置
              </th><th class="p-2">
                合成结果
              </th><th class="p-2">
                真实模型 / 客户端
              </th><th class="p-2" />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="item in runs"
              :key="item.run.id"
              class="border-b"
              :class="{ 'bg-muted/40': selectedRun?.id === item.run.id }"
            >
              <td class="whitespace-nowrap p-2">
                {{ date(item.run.created_at) }}
              </td><td class="max-w-40 break-all p-2">
                {{ clientLabel(item.run.config.client) }}<div class="text-muted-foreground">
                  {{ item.run.config.client_version }}
                </div>
              </td>
              <td class="max-w-56 break-all p-2">
                {{ item.run.config.model }}<div class="font-mono text-muted-foreground">
                  {{ item.run.config.client_protocol }} → {{ item.run.config.upstream_format }}
                </div>
              </td>
              <td class="p-2">
                <span :class="item.stale !== false ? 'text-amber-700 dark:text-amber-400' : 'text-muted-foreground'">{{ item.stale === true ? '已过期' : item.stale === false ? '当前指纹' : '未知（非当前）' }}</span><div
                  class="font-mono"
                  :title="item.run.config_fingerprint"
                >
                  {{ item.run.config_fingerprint.slice(0, 12) }}
                </div>
              </td>
              <td class="p-2">
                {{ item.run.evidence === 'offline_synthetic' ? compatibilityStatus(runSummary(item.run.cases)) : '未知证据（非通过）' }}
              </td><td class="p-2">
                未测 / 未测
              </td>
              <td class="p-2 text-right">
                <Button
                  variant="ghost"
                  size="sm"
                  @click="selectedRun = item.run"
                >
                  逐项结果
                </Button>
              </td>
            </tr><tr v-if="!runs.length">
              <td
                colspan="7"
                class="p-6 text-center text-muted-foreground"
              >
                暂无匹配记录
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
    <section
      v-if="selectedRun"
      class="min-w-0 space-y-2 border-t pt-4"
    >
      <div class="flex flex-wrap items-center gap-3">
        <h2 class="text-sm font-semibold">
          逐项结果
        </h2><code class="break-all text-xs text-muted-foreground">{{ selectedRun.id }}</code>
        <span class="text-xs text-muted-foreground">{{ selectedStale === true ? '已过期' : selectedStale === false ? '当前指纹' : '未知（非当前）' }}</span>
      </div>
      <table class="w-full table-fixed text-left text-xs">
        <thead class="border-b text-muted-foreground">
          <tr>
            <th class="w-32 p-2">
              场景
            </th><th class="w-24 p-2">
              状态
            </th><th class="p-2">
              原因
            </th>
          </tr>
        </thead><tbody>
          <tr
            v-for="item in selectedRun.cases"
            :key="item.case"
            class="border-b align-top"
          >
            <td class="break-words p-2">
              {{ caseLabel(item.case) }}
            </td><td
              class="break-words p-2"
              :class="{ 'text-destructive': item.status === 'failed' }"
            >
              {{ compatibilityStatus(item.status) }}
            </td><td class="break-words p-2 text-muted-foreground">
              {{ item.reason }}
            </td>
          </tr>
        </tbody>
      </table>
    </section>
  </PageContainer>
</template>
