<template>
  <section
    class="border-t py-5 min-w-0"
    aria-label="权益生效预览"
  >
    <div class="flex flex-wrap items-center justify-between gap-3">
      <h2 class="text-base font-semibold">
        权益生效预览
      </h2>
      <Button
        variant="outline"
        :disabled="!userId || busy || disabled"
        @click="load"
      >
        <Eye class="h-4 w-4" />预览当前草稿
      </Button>
    </div>
    <div class="mt-4 grid gap-3 sm:grid-cols-2">
      <label class="text-sm space-y-2"><span class="block text-muted-foreground">用户</span><select
        v-model="userId"
        aria-label="预览用户"
        class="h-10 w-full border rounded-md bg-background px-3"
        :disabled="busy"
      ><option value="">请选择用户</option><option
        v-for="user in users"
        :key="user.id"
        :value="user.id"
      >{{ user.name }}</option></select></label>
      <label class="text-sm space-y-2"><span class="block text-muted-foreground">API Key（可选）</span><select
        v-model="keyId"
        aria-label="预览 API Key"
        class="h-10 w-full border rounded-md bg-background px-3"
        :disabled="busy || keysLoading || !userId"
      ><option value="">仅用户权限</option><option
        v-for="key in keys"
        :key="key.id"
        :value="key.id"
      >{{ key.name || key.id }}{{ !key.is_active ? '（已停用）' : '' }}</option></select></label>
    </div>
    <p
      v-if="keyError"
      role="alert"
      class="text-sm text-destructive mt-3"
    >
      {{ keyError }}
    </p>
    <p
      v-if="error"
      role="alert"
      class="text-sm text-destructive mt-3"
    >
      {{ error }}
    </p>
    <template v-if="result">
      <p
        v-if="!result.enabled"
        role="status"
        class="mt-3 text-sm text-muted-foreground"
      >
        内部权益限制未启用
      </p>
      <div class="mt-4 flex flex-wrap gap-x-5 gap-y-2 text-sm">
        <span>{{ result.user.username }} · {{ result.user.is_active ? '账户有效' : '账户已停用' }}</span>
        <span>模型权限：{{ result.model_access.source === 'personal' ? '个人配置' : '组继承' }}</span>
        <span v-if="result.key">Key：{{ result.key.currently_usable ? '有效' : '不可使用' }}</span>
        <span>{{ result.changed ? '草稿将改变此用户权益' : '草稿未改变此用户权益' }}</span>
      </div>
      <p class="mt-3 text-sm break-all">
        有效模型范围：{{ modelLabel(result.key ? result.key.allowed_models : result.model_access.allowed_models) }}
      </p>
      <div class="mt-4 grid gap-6 lg:grid-cols-2">
        <div
          v-for="scenario in scenarios"
          :key="scenario.title"
          class="min-w-0"
        >
          <h3 class="font-medium text-sm">
            {{ scenario.title }} · {{ sourceLabels[scenario.value.source] }}
          </h3>
          <span class="text-xs text-muted-foreground">{{ scenario.value.quota.timezone }}{{ scenario.value.quota.unlimited ? ' · 不限额' : '' }}</span>
          <div class="overflow-x-auto mt-2">
            <table class="w-full text-xs text-left">
              <thead class="border-b text-muted-foreground">
                <tr>
                  <th class="p-2">
                    池 / 周期
                  </th><th class="p-2">
                    上限 USD
                  </th><th class="p-2">
                    已用 / 占用
                  </th><th class="p-2">
                    剩余 USD
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="window in scenario.value.quota.windows"
                  :key="`${window.family ? `family:${window.family}` : window.model ? `model:${window.model}` : 'total'}:${window.period}`"
                  class="border-b"
                >
                  <td class="p-2 break-all min-w-24">
                    {{ window.family_name || window.model || '总池' }}<br><span class="text-muted-foreground">{{ periods[window.period] }}</span>
                  </td><td class="p-2">
                    {{ money(window.limit_usd) }}
                  </td><td class="p-2">
                    {{ money(window.used_usd) }} / {{ money(window.reserved_usd) }}
                  </td><td class="p-2">
                    {{ money(window.remaining_usd) }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </template>
  </section>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { Eye } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { previewAccess, type AccessPreview } from '@/api/access-preview'
import { usersApi, type ApiKey } from '@/api/users'
import type { InternalAccessConfig } from '@/api/internal-access'
import { parseApiError } from '@/utils/errorParser'
const props = defineProps<{ users: { id: string; name: string }[]; getDraft: () => InternalAccessConfig; selectedUser?: string; disabled?: boolean }>()
const userId = ref(''), keyId = ref(''), keys = ref<ApiKey[]>([]), result = ref<AccessPreview>()
const busy = ref(false), keysLoading = ref(false), error = ref(''), keyError = ref('')
let version = 0, keysVersion = 0, controller: AbortController | undefined
const periods = { five_hour: '滚动 5 小时', daily: '每日', weekly: '每周', monthly: '每月' }
const sourceLabels = { personal: '个人覆盖', group: '组权益', default: '默认权益' }
const scenarios = computed(() => result.value ? [{ title: '已保存', value: result.value.current }, ...(result.value.proposed ? [{ title: '待保存', value: result.value.proposed }] : [])] : [])
const money = (value: number | null) => value === null ? '不限' : new Intl.NumberFormat('en-US', { maximumFractionDigits: 4 }).format(value)
const modelLabel = (values: string[] | null) => values === null ? '不限模型（仍受模型状态及渠道可用性限制）' : values.length ? values.join('、') : '无可用模型'
watch(() => props.selectedUser, value => { if (value) userId.value = value }, { immediate: true })
watch(userId, async value => {
  const mine = ++keysVersion
  keyId.value = ''; keys.value = []; keyError.value = ''
  if (!value) return
  keysLoading.value = true
  try { const data = await usersApi.getUserApiKeys(value); if (mine === keysVersion) keys.value = data }
  catch { if (mine === keysVersion) keyError.value = '无法加载此用户的 Key，仅可预览用户权限' }
  finally { if (mine === keysVersion) keysLoading.value = false }
}, { immediate: true })
function draftFingerprint() { try { return JSON.stringify(props.getDraft()) } catch { return 'invalid' } }
watch([userId, keyId, draftFingerprint], () => { ++version; controller?.abort(); result.value = undefined; error.value = ''; busy.value = false })
async function load() {
  const mine = ++version
  controller?.abort(); controller = new AbortController()
  busy.value = true; error.value = ''; result.value = undefined
  try { const data = await previewAccess(userId.value, props.getDraft(), keyId.value || undefined, controller.signal); if (mine === version) result.value = data }
  catch (cause) { if (mine === version) error.value = parseApiError(cause, cause instanceof Error ? cause.message : '预览权益失败') }
  finally { if (mine === version) busy.value = false }
}
onUnmounted(() => { ++version; ++keysVersion; controller?.abort() })
</script>
