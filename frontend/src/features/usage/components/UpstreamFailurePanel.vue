<template>
  <section
    class="min-w-0 space-y-3 border-y py-4"
    aria-label="上游失败对比"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="text-sm font-semibold">
        上游失败对比
      </h3>
      <div
        v-if="attempts.length > 1"
        class="flex flex-wrap gap-1"
        role="tablist"
        aria-label="失败的尝试"
      >
        <button
          v-for="(attempt, index) in attempts"
          :key="attempt.id"
          type="button"
          role="tab"
          :aria-selected="index === selected"
          class="rounded-md border px-2 py-0.5 text-xs"
          :class="index === selected ? 'border-primary bg-primary/10 text-foreground' : 'text-muted-foreground'"
          @click="selected = index"
        >
          {{ upstreamErrorView(attempt).attemptLabel }}
        </button>
      </div>
    </div>

    <p
      v-if="loading"
      role="status"
      class="text-sm text-muted-foreground"
    >
      读取中...
    </p>
    <p
      v-if="error"
      role="alert"
      class="text-sm text-destructive"
    >
      {{ error }}
    </p>
    <p
      v-if="!loading && !error && !view"
      class="text-sm text-muted-foreground"
    >
      这个请求没有到达上游的失败记录（可能在网关内被拒绝，或尚未执行）。
    </p>

    <div
      v-if="view"
      class="grid min-w-0 gap-4 lg:grid-cols-2"
    >
      <div class="min-w-0 space-y-2">
        <div class="flex items-center justify-between gap-2">
          <h4 class="text-xs font-medium text-muted-foreground">
            转换后发给上游的请求
          </h4>
          <Button
            v-if="requestJson"
            size="sm"
            variant="outline"
            @click="copy(requestJson.text)"
          >
            <Copy class="mr-1 h-3.5 w-3.5" />复制
          </Button>
        </div>
        <p
          v-if="view.formats"
          class="break-all font-mono text-[11px] text-muted-foreground"
        >
          {{ view.formats }}
        </p>
        <pre
          v-if="requestJson"
          class="max-h-80 overflow-auto rounded-md border bg-muted/30 p-3 font-mono text-xs"
          data-testid="upstream-request-body"
        >{{ requestJson.text }}</pre>
        <p
          v-if="sentBody?.passthrough"
          class="text-xs text-muted-foreground"
          data-testid="upstream-request-passthrough"
        >
          同格式直通：网关只保存了一份请求体（客户端发来的），发往上游的内容与它相同，仅模型名可能被映射。
        </p>
        <p
          v-if="requestJson?.truncated"
          class="text-xs text-muted-foreground"
        >
          内容过长，仅显示前 {{ MAX_PANEL_JSON_CHARS.toLocaleString() }} 个字符；完整内容见下方请求体标签页。
        </p>
        <p
          v-if="availability === 'disabled'"
          class="rounded-md border border-dashed p-3 text-xs text-muted-foreground"
          data-testid="upstream-request-disabled"
        >
          这条请求没有保存发给上游的请求体（记录级别为 BASIC）。到「系统设置 → 请求记录」改为 FAILURES，之后再失败的请求会自动保存；成功请求仍不留正文。
        </p>
        <p
          v-else-if="availability === 'missing'"
          class="rounded-md border border-dashed p-3 text-xs text-muted-foreground"
        >
          没有找到发给上游的请求体。
        </p>
      </div>

      <div class="min-w-0 space-y-2">
        <div class="flex items-center justify-between gap-2">
          <h4 class="text-xs font-medium text-muted-foreground">
            上游返回的错误
          </h4>
          <Button
            size="sm"
            variant="outline"
            @click="copy(responseText)"
          >
            <Copy class="mr-1 h-3.5 w-3.5" />复制
          </Button>
        </div>
        <p class="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
          <span
            class="font-mono font-semibold text-destructive"
            data-testid="upstream-status"
          >{{ view.statusCode ?? '无状态码' }}</span>
          <span class="text-muted-foreground">{{ view.providerName }}</span>
          <span
            v-if="view.decision"
            class="text-xs text-muted-foreground"
          >网关决定：{{ decisionLabel(view.decision) }}<template v-if="view.retryable !== null">（{{ view.retryable ? '可重试' : '不可重试' }}）</template></span>
        </p>
        <p
          v-if="view.upstreamUrl"
          class="break-all font-mono text-[11px] text-muted-foreground"
        >
          {{ view.upstreamUrl }}
        </p>
        <dl
          v-if="view.headers.length"
          class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-xs"
        >
          <template
            v-for="[name, value] in view.headers"
            :key="name"
          >
            <dt class="font-mono text-muted-foreground">
              {{ name }}
            </dt>
            <dd class="break-all font-mono">
              {{ value }}
            </dd>
          </template>
        </dl>
        <pre
          v-if="responseJson.text"
          class="max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-md border bg-muted/30 p-3 font-mono text-xs"
          data-testid="upstream-error-body"
        >{{ responseJson.text }}</pre>
        <p
          v-else-if="view.message"
          class="break-words text-xs"
        >
          {{ view.message }}
        </p>
        <p
          v-if="responseJson.truncated"
          class="text-xs text-muted-foreground"
        >
          内容过长，已截断。
        </p>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { Copy } from 'lucide-vue-next'
import Button from '@/components/ui/button.vue'
import { dashboardApi, type RequestDetail } from '@/api/dashboard'
import { requestTraceApi, type RequestTrace } from '@/api/requestTrace'
import { useClipboard } from '@/composables/useClipboard'
import {
  MAX_PANEL_JSON_CHARS,
  boundedJson,
  failedAttempts,
  upstreamErrorView,
  upstreamRequestAvailability,
} from '../utils/upstream-failure'

const props = defineProps<{ requestId: string | null, usageId: string | null }>()

const trace = ref<RequestTrace | null>(null)
const detail = ref<RequestDetail | null>(null)
const loading = ref(false)
const error = ref('')
const selected = ref(0)
let generation = 0
let controller: AbortController | undefined
const { copyToClipboard } = useClipboard()

const attempts = computed(() => failedAttempts(trace.value))
// The last failed attempt is the one that decided the outcome, so it is shown first.
const view = computed(() => {
  const attempt = attempts.value[selected.value]
  return attempt ? upstreamErrorView(attempt, detail.value?.response_body) : null
})
// Same-format passthrough stores a single copy: the client's request is what went upstream.
const sentBody = computed(() => {
  const converted = detail.value?.provider_request_body
  if (converted != null) return { body: converted, passthrough: false }
  if (detail.value?.has_format_conversion === false && detail.value.request_body != null) {
    return { body: detail.value.request_body, passthrough: true }
  }
  return null
})
const requestJson = computed(() => (sentBody.value ? boundedJson(sentBody.value.body) : null))
const responseJson = computed(() => view.value && view.value.body != null
  ? boundedJson(view.value.body)
  : { text: '', truncated: false })
const responseText = computed(() => responseJson.value.text || view.value?.message || '')
const availability = computed(() => upstreamRequestAvailability(
  sentBody.value?.body,
  detail.value?.has_provider_request_body,
  trace.value?.diagnostic_request?.body_state,
))

const DECISIONS: Record<string, string> = {
  retry_next_candidate: '换下一个候选重试',
  stop: '停止并返回错误',
  return_error: '返回错误',
}
const decisionLabel = (decision: string) => DECISIONS[decision] ?? decision

async function copy(text: string) {
  if (text) await copyToClipboard(text)
}

async function load() {
  generation += 1
  const current = generation
  controller?.abort()
  trace.value = null
  detail.value = null
  error.value = ''
  if (!props.requestId) return
  controller = new AbortController()
  loading.value = true
  try {
    const loadedTrace = await requestTraceApi.getRequestTrace(props.requestId, { attemptedOnly: true })
    if (current !== generation) return
    trace.value = loadedTrace
    selected.value = Math.max(0, failedAttempts(loadedTrace).length - 1)
    const usageId = props.usageId || loadedTrace.diagnostic_request?.usage_id
    if (usageId) {
      const loadedDetail = await dashboardApi.getRequestDetail(usageId, { includeBodies: true, signal: controller.signal })
      if (current === generation) detail.value = loadedDetail
    }
  } catch {
    if (current === generation) error.value = '无法读取上游失败详情，请检查权限或稍后重试'
  } finally {
    if (current === generation) loading.value = false
  }
}

watch(() => [props.requestId, props.usageId], load, { immediate: true })
onBeforeUnmount(() => {
  generation += 1
  controller?.abort()
})
</script>
