<template>
  <PageContainer>
    <PageHeader
      title="Agent MCP"
      :icon="Bot"
    >
      <template #actions>
        <Button
          variant="outline"
          size="icon"
          title="刷新"
          :disabled="loading || busy"
          @click="load"
        >
          <RefreshCw class="h-4 w-4" />
        </Button>
        <Button
          :disabled="busy || !config?.enabled"
          @click="openCreate"
        >
          <Plus class="mr-2 h-4 w-4" />创建 API Key
        </Button>
      </template>
    </PageHeader>
    <p
      v-if="error"
      role="alert"
      class="text-sm text-destructive"
    >
      {{ error }}
    </p>
    <section class="grid gap-5 border-y border-border py-5 md:grid-cols-2">
      <div class="min-w-0 space-y-2">
        <h2 class="text-sm font-semibold">
          连接端点
        </h2>
        <div class="flex min-w-0 items-center gap-2">
          <code class="break-all text-sm">{{ endpoint }}</code><Button
            variant="ghost"
            size="icon"
            title="复制端点"
            @click="copy(endpoint)"
          >
            <Copy class="h-4 w-4" />
          </Button>
        </div>
        <span class="text-xs text-muted-foreground">{{ config ? (config.enabled ? '已启用' : '已停用') : '未连接' }} · Streamable HTTP · Bearer API Key</span>
      </div>
      <div class="space-y-2">
        <h2 class="text-sm font-semibold">
          当前身份
        </h2>
        <p class="text-sm">
          {{ config ? adminRoleLabel(config.role) : '—' }}
        </p>
        <p class="break-all font-mono text-xs text-muted-foreground">
          {{ config?.user_id || '—' }}
        </p>
      </div>
    </section>
    <section class="min-w-0 space-y-4">
      <h2 class="text-sm font-semibold">
        我的 MCP API Key
      </h2>
      <p
        v-if="loading"
        role="status"
        class="py-8 text-center text-sm text-muted-foreground"
      >
        加载中…
      </p>
      <p
        v-else-if="config && !config.items.length"
        class="py-8 text-center text-sm text-muted-foreground"
      >
        暂无 API Key
      </p>
      <div
        v-else-if="config"
        class="overflow-x-auto"
      >
        <table class="w-full text-left text-sm">
          <thead class="border-b text-xs text-muted-foreground">
            <tr>
              <th class="p-3">
                名称
              </th><th class="p-3">
                权限
              </th><th class="p-3">
                状态
              </th><th class="p-3">
                到期
              </th><th class="p-3">
                最近使用
              </th><th class="p-3 text-right">
                操作
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="key in config.items"
              :key="key.id"
              class="border-b"
            >
              <td class="p-3">
                <div class="max-w-64 break-words font-medium">
                  {{ key.name.replace(/^MCP\//, '') }}
                </div><code class="text-xs text-muted-foreground">{{ key.token_prefix }}…</code>
              </td><td class="p-3">
                {{ key.permissions?.every(p => p.endsWith(':read')) ? '只读' : '读写' }}<span class="ml-1 text-xs text-muted-foreground">{{ key.permissions?.length || 0 }} 项</span>
              </td><td class="p-3">
                {{ active(key) ? '有效' : '已失效' }}
              </td><td class="whitespace-nowrap p-3">
                {{ key.expires_at_unix_secs == null ? '永不过期' : date(key.expires_at_unix_secs) }}
              </td><td class="whitespace-nowrap p-3">
                {{ date(key.last_used_at_unix_secs) }}
              </td><td class="p-3 text-right">
                <Button
                  variant="ghost"
                  :title="`修改有效期 ${key.name}`"
                  :disabled="busy"
                  @click="openEdit(key)"
                >
                  修改有效期
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  :title="`撤销 ${key.name}`"
                  :disabled="busy"
                  @click="openRevoke(key)"
                >
                  <Trash2 class="h-4 w-4 text-destructive" />
                </Button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
    <section class="space-y-3 border-t border-border pt-5">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold">
          客户端配置
        </h2><Button
          variant="ghost"
          size="icon"
          title="复制配置"
          @click="copy(clientConfig)"
        >
          <Copy class="h-4 w-4" />
        </Button>
      </div>
      <pre class="overflow-x-auto rounded-md bg-muted/40 p-4 text-xs leading-6">{{ clientConfig }}</pre>
    </section>
    <Dialog
      :open="creating"
      title="创建 MCP API Key"
      size="md"
      :persistent="busy"
      @update:open="!busy && (creating = $event)"
    >
      <form
        class="space-y-5"
        @submit.prevent="create"
      >
        <div class="space-y-2">
          <label
            for="mcp-key-name"
            class="text-sm font-medium"
          >名称</label><Input
            id="mcp-key-name"
            v-model="name"
            required
            :maxlength="80"
            autocomplete="off"
          />
        </div>
        <fieldset class="space-y-2">
          <legend class="mb-2 text-sm font-medium">
            权限
          </legend><div class="flex gap-5">
            <label class="flex items-center gap-2 text-sm"><input
              v-model="mode"
              type="radio"
              value="read"
            >只读</label><label class="flex items-center gap-2 text-sm"><input
              v-model="mode"
              type="radio"
              value="write"
              :disabled="!canWrite"
            >读写</label>
          </div>
        </fieldset>
        <label
          class="block space-y-2 text-sm"
          for="mcp-key-purpose"
        ><span>授权范围</span><select
          id="mcp-key-purpose"
          v-model="purpose"
          class="h-10 w-full rounded-md border bg-background px-3"
        ><option value="role">当前角色权限</option><option
          value="feedback"
          :disabled="!config?.read_permissions.includes('admin:feedback:read')"
        >仅讨论与反馈</option></select></label>
        <label class="flex items-center gap-2 text-sm">
          <input
            id="mcp-key-never"
            v-model="neverExpires"
            type="checkbox"
          >
          永不过期
        </label>
        <p
          v-if="neverExpires"
          class="text-xs text-muted-foreground"
        >
          永久密钥需要定期检查，不再使用时请及时撤销。
        </p>
        <div
          v-else
          class="space-y-2"
        >
          <label
            for="mcp-key-days"
            class="text-sm font-medium"
          >有效期（天）</label><Input
            id="mcp-key-days"
            v-model="days"
            type="number"
            min="1"
            max="90"
            required
          />
        </div>
        <div class="space-y-2">
          <label
            for="mcp-key-ips"
            class="text-sm font-medium"
          >IP 白名单（可选）</label><Input
            id="mcp-key-ips"
            v-model="ips"
            placeholder="203.0.113.1, 10.0.0.0/24"
            autocomplete="off"
          />
        </div>
        <p
          v-if="dialogError"
          role="alert"
          class="text-sm text-destructive"
        >
          {{ dialogError }}
        </p>
        <div class="flex justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            :disabled="busy"
            @click="creating = false"
          >
            取消
          </Button><Button
            type="submit"
            :disabled="busy || !name.trim()"
          >
            创建
          </Button>
        </div>
      </form>
    </Dialog>
    <Dialog
      :open="!!editing"
      title="修改有效期"
      size="sm"
      :persistent="busy"
      @update:open="!busy && (editing = undefined)"
    >
      <form
        class="space-y-4"
        @submit.prevent="updateExpiry"
      >
        <p class="break-all text-sm">
          {{ editing?.name }}
        </p>
        <p class="text-xs text-muted-foreground">
          有效期从保存时起计算；不会更改权限或重新启用已停用的密钥。
        </p>
        <label class="flex items-center gap-2 text-sm">
          <input
            id="mcp-edit-never"
            v-model="editNeverExpires"
            type="checkbox"
          >永不过期
        </label>
        <div
          v-if="!editNeverExpires"
          class="space-y-2"
        >
          <label
            for="mcp-edit-days"
            class="text-sm font-medium"
          >有效期（天）</label>
          <Input
            id="mcp-edit-days"
            v-model="editDays"
            type="number"
            min="1"
            max="90"
            required
          />
        </div>
        <p
          v-if="editNeverExpires"
          class="text-xs text-muted-foreground"
        >
          永久密钥需要定期检查，不再使用时请及时撤销。
        </p>
        <p
          v-if="dialogError"
          role="alert"
          class="text-sm text-destructive"
        >
          {{ dialogError }}
        </p>
        <div class="flex justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            :disabled="busy"
            @click="editing = undefined"
          >
            取消
          </Button>
          <Button
            type="submit"
            :disabled="busy"
          >
            保存
          </Button>
        </div>
      </form>
    </Dialog>
    <Dialog
      :open="!!secret"
      title="API Key（仅显示一次）"
      size="md"
      @update:open="secret = ''"
    >
      <div class="space-y-4">
        <code
          class="block break-all rounded-md bg-muted p-4 text-sm"
          data-testid="mcp-new-secret"
        >{{ secret }}</code><div class="flex justify-end gap-2">
          <Button
            variant="outline"
            @click="copy(secret)"
          >
            <Copy class="mr-2 h-4 w-4" />复制
          </Button><Button @click="secret = ''">
            关闭
          </Button>
        </div>
      </div>
    </Dialog>
    <Dialog
      :open="!!revoking"
      title="撤销 API Key"
      size="sm"
      :persistent="busy"
      @update:open="!busy && (revoking = undefined)"
    >
      <div class="space-y-4">
        <p class="break-all text-sm">
          {{ revoking?.name }} 的访问权限将立即失效。
        </p>
        <p
          v-if="dialogError"
          role="alert"
          class="text-sm text-destructive"
        >
          {{ dialogError }}
        </p>
        <div class="flex justify-end gap-2">
          <Button
            variant="outline"
            :disabled="busy"
            @click="revoking = undefined"
          >
            取消
          </Button><Button
            variant="destructive"
            :disabled="busy"
            @click="revoke"
          >
            撤销
          </Button>
        </div>
      </div>
    </Dialog>
  </PageContainer>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Bot, Copy, Plus, RefreshCw, Trash2 } from 'lucide-vue-next'
import PageContainer from '@/components/layout/PageContainer.vue'
import PageHeader from '@/components/layout/PageHeader.vue'
import { Button, Input } from '@/components/ui'
import Dialog from '@/components/ui/dialog/Dialog.vue'
import { adminRoleLabel } from '@/config/adminRoles'
import { listMcpKeys, createMcpKey, revokeMcpKey, updateMcpKey, type McpKey, type McpKeysResponse } from '@/api/mcp'

import { extractErrorMessage } from '@/utils/error'

const config = ref<McpKeysResponse>()
const loading = ref(false), busy = ref(false), creating = ref(false)
const error = ref(''), dialogError = ref(''), secret = ref('')
const name = ref(''), mode = ref('read'), days = ref<number | string>(30), ips = ref('')
const purpose = ref('role')
const revoking = ref<McpKey>(), editing = ref<McpKey>()
const neverExpires = ref(false), editNeverExpires = ref(false), editDays = ref<number | string>(30)
const endpoint = computed(() => new URL(config.value?.endpoint || '/mcp', window.location.origin).href)
const canWrite = computed(() => config.value?.write_permissions.some(p => !p.endsWith(':read')) ?? false)
const clientConfig = computed(() => JSON.stringify({ mcpServers: { arca: { url: endpoint.value, headers: { Authorization: 'Bearer <API_KEY>' } } } }, null, 2))
function date(value?: number | null) { return value ? new Date(value * 1000).toLocaleString() : '—' }
function active(key: McpKey) { return key.is_active && (!key.expires_at_unix_secs || key.expires_at_unix_secs * 1000 > Date.now()) }
async function copy(value: string) { try { await navigator.clipboard.writeText(value) } catch { error.value = '复制失败' } }
function openCreate() { if (busy.value) return; dialogError.value = ''; creating.value = true }
function openRevoke(key: McpKey) { if (busy.value) return; dialogError.value = ''; revoking.value = key }
function openEdit(key: McpKey) {
  if (busy.value) return
  dialogError.value = ''; editing.value = key; editNeverExpires.value = key.expires_at_unix_secs == null; editDays.value = 30
}
function validDays(value: number | string) { return Number.isInteger(Number(value)) && Number(value) >= 1 && Number(value) <= 90 }
async function load() {
  loading.value = true; error.value = ''
  try { config.value = await listMcpKeys(); return true }
  catch { config.value = undefined; error.value = 'MCP 后端未连接或当前身份无权访问'; return false }
  finally { loading.value = false }
}
async function refreshAfterMutation(message: string) {
  if (!await load()) error.value = `${message}，但列表刷新失败，请重试刷新。`
}
async function create() {
  if (busy.value) return
  dialogError.value = ''
  if (!neverExpires.value && !validDays(days.value)) { dialogError.value = '有效期必须为 1–90 天的整数'; return }
  busy.value = true
  try {
    const permissions = purpose.value === 'feedback' ? (mode.value === 'write' ? config.value?.write_permissions : config.value?.read_permissions)?.filter(p => p.startsWith('admin:feedback:') && !p.endsWith(':admin')) : undefined
    const result = await createMcpKey({ name: name.value.trim(), read_only: mode.value !== 'write', expires_in_days: neverExpires.value ? null : Number(days.value), allowed_ips: ips.value.trim() ? ips.value.split(',').map(s => s.trim()).filter(Boolean) : undefined, permissions })
    secret.value = result.token; creating.value = false; name.value = ''; mode.value = 'read'; purpose.value = 'role'; ips.value = ''; days.value = 30; neverExpires.value = false
    await refreshAfterMutation('密钥已创建')
  } catch (err) { dialogError.value = extractErrorMessage(err, '创建失败，请检查权限、有效期或 IP 白名单') }
  finally { busy.value = false }
}
async function updateExpiry() {
  if (busy.value || !editing.value) return
  dialogError.value = ''
  if (!editNeverExpires.value && !validDays(editDays.value)) { dialogError.value = '有效期必须为 1–90 天的整数'; return }
  busy.value = true
  try {
    await updateMcpKey(editing.value.id, { expires_in_days: editNeverExpires.value ? null : Number(editDays.value) })
    editing.value = undefined
    await refreshAfterMutation('有效期已更新')
  } catch (err) { dialogError.value = extractErrorMessage(err, '修改有效期失败') }
  finally { busy.value = false }
}
async function revoke() {
  if (busy.value || !revoking.value) return
  busy.value = true; dialogError.value = ''
  try {
    await revokeMcpKey(revoking.value.id); revoking.value = undefined
    await refreshAfterMutation('密钥已撤销')
  } catch (err) { dialogError.value = extractErrorMessage(err, '撤销失败') }
  finally { busy.value = false }
}
onMounted(load)
</script>
