<template>
  <section
    class="min-w-0 space-y-3 border-y py-4"
    aria-label="请求故障诊断"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="text-sm font-semibold">
        请求诊断
      </h3>
      <div class="flex gap-2">
        <Button
          size="icon"
          variant="outline"
          :title="diagnostic ? '刷新诊断' : '读取诊断'"
          :disabled="loading || !requestId"
          @click="load"
        >
          <RefreshCw
            v-if="diagnostic"
            class="h-4 w-4"
          /><FileSearch
            v-else
            class="h-4 w-4"
          />
        </Button>
        <Button
          v-if="diagnostic"
          size="icon"
          variant="outline"
          title="下载安全诊断包"
          @click="download"
        >
          <Download class="h-4 w-4" />
        </Button>
        <Button
          v-if="diagnostic && !hideFeedback"
          variant="outline"
          :disabled="diagnostic.state === 'unknown'"
          @click="emit('feedback', diagnostic.request_id)"
        >
          <MessagesSquare class="mr-2 h-4 w-4" />{{ admin ? '关联反馈' : '带入反馈' }}
        </Button>
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
    <template v-if="diagnostic">
      <p class="break-all font-mono text-xs text-muted-foreground">
        {{ diagnostic.request_id }} · {{ stateLabels[diagnostic.state] }}
      </p>
      <p
        v-if="diagnostic.has_attempt_failures"
        class="text-xs text-muted-foreground"
      >
        存在失败或跳过的候选尝试；请求最终结果：{{ outcomeLabels[diagnostic.final_outcome] }}
      </p>
      <dl class="grid min-w-0 gap-x-6 gap-y-3 sm:grid-cols-2">
        <div
          v-for="stage in diagnostic.stages"
          :key="stage.id"
          class="min-w-0 border-b pb-2"
        >
          <dt class="flex justify-between gap-3 text-sm">
            <span>{{ stageLabels[stage.id] || stage.id }}</span><span :class="stage.status === 'failed' ? 'text-destructive' : 'text-muted-foreground'">{{ statusLabels[stage.status] }}</span>
          </dt>
          <dd class="mt-1 break-words text-xs text-muted-foreground">
            {{ codeLabels[stage.code] || stage.code }}
          </dd>
          <dd
            v-if="stage.evidence.length"
            class="mt-1 break-all font-mono text-[11px] text-muted-foreground"
          >
            {{ stage.evidence.join(', ') }}
          </dd>
        </div>
      </dl>
    </template>
  </section>
</template>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { Download, FileSearch, MessagesSquare, RefreshCw } from 'lucide-vue-next'
import Button from '@/components/ui/button.vue'
import { getRequestDiagnostic, type RequestDiagnostic } from '@/api/diagnostics'

const props = defineProps<{ requestId: string | null; admin?: boolean; hideFeedback?: boolean }>()
const emit = defineEmits<{ feedback: [requestId: string] }>()
const diagnostic = ref<RequestDiagnostic>(), loading = ref(false), error = ref('')
let generation = 0, controller: AbortController | undefined
const statusLabels = { known: '已知', failed: '失败', unknown: '未知' }
const stateLabels = { known: '已记录完成', failed: '有失败记录', unknown: '最终结果未知', in_progress: '运行中，结果未知' }
const outcomeLabels = { unknown: '未知', in_progress: '运行中', completed: '已完成', failed: '失败', cancelled: '已取消' }
const stageLabels: Record<string, string> = { identity: '身份', model_acl: '模型 ACL', quota: '额度', candidate_filter: '候选筛选', conversion: '格式转换', tunnel: '隧道 / 代理传输', upstream: '上游', stream: '流' }
const codeLabels: Record<string, string> = {
  not_recorded: '无独立阶段证据', owner_recorded: '已记录请求归属', candidates_recorded: '已记录候选筛选', candidate_selected: '已记录所选候选', conversion_configured: '已配置转换，未证明转换成功',
  first_byte_recorded: '已记录首字节，未证明完整响应成功', completion_recorded: '已记录请求完成', plan_usage_limit_exceeded: '已记录额度拦截', model_access_denied: '已记录模型访问拒绝',
  no_eligible_candidates: '已记录无可用候选', candidate_rejected: '已记录候选被筛除', format_conversion_failed: '已记录格式转换失败', proxy_transport_failed: '已记录代理传输错误', stream_failed_or_interrupted: '已记录流失败或中断',
  invalid_request_error: '已记录 invalid_request_error', upstream_format_error: '已记录上游格式 / 解码错误', upstream_transport_error: '已记录上游传输错误', upstream_error_recorded: '已记录上游错误',
}
function reset() { generation++; controller?.abort(); diagnostic.value = undefined; error.value = ''; loading.value = false }
watch(() => [props.requestId, props.admin], reset)
onBeforeUnmount(reset)
async function load() {
  reset()
  if (!props.requestId) return
  const current = generation
  controller = new AbortController(); loading.value = true
  try { const result = await getRequestDiagnostic(Boolean(props.admin), props.requestId, controller.signal); if (current === generation) diagnostic.value = result }
  catch { if (current === generation) error.value = '无法读取诊断，请检查访问权限或稍后重试' }
  finally { if (current === generation) loading.value = false }
}
function download() {
  if (!diagnostic.value) return
  const url = URL.createObjectURL(new Blob([JSON.stringify(diagnostic.value, null, 2)], { type: 'application/json' }))
  const link = document.createElement('a'); link.href = url; link.download = 'request-diagnostic.json'; link.click(); URL.revokeObjectURL(url)
}
</script>
