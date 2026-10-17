<template>
  <div class="profile-page">
    <header class="profile-heading">
      <UserAvatar
        :user-id="auth.user?.id"
        :version="auth.user?.avatar_version"
        :name="profile?.username || auth.user?.username"
        :size="48"
      />
      <div class="min-w-0 flex-1">
        <h1 class="text-xl font-semibold">
          个人资料
        </h1>
        <p
          v-if="profile"
          class="text-sm text-muted-foreground mt-1 break-words"
        >
          {{ profile.username }}<span v-if="profile.email"> · {{ profile.email }}</span>
        </p>
        <p
          v-else-if="profileError"
          role="alert"
          class="text-sm text-destructive"
        >
          个人信息加载失败 <button
            class="underline"
            @click="loadProfile"
          >
            重试
          </button>
        </p>
        <p
          v-else
          class="text-sm text-muted-foreground mt-1"
        >
          加载账户信息…
        </p>
      </div>
      <span
        v-if="profile"
        class="profile-status"
      >{{ profile.is_active ? '账户正常' : '账户停用' }}</span>
    </header>

    <div class="profile-layout">
      <nav
        class="profile-nav"
        aria-label="个人资料分类"
      >
        <button
          v-for="item in sections"
          :key="item.id"
          :aria-current="section === item.id ? 'page' : undefined"
          @click="section = item.id"
        >
          <component
            :is="item.icon"
            :size="17"
            aria-hidden="true"
          /><span>{{ item.label }}</span>
        </button>
        <RouterLink to="/dashboard/api-keys">
          <KeyRound
            :size="17"
            aria-hidden="true"
          /><span>API 密钥</span><ArrowUpRight
            :size="14"
            class="ml-auto"
            aria-hidden="true"
          />
        </RouterLink>
      </nav>

      <div class="profile-content">
        <template v-if="section === 'usage'">
          <section aria-labelledby="entitlement-title">
            <div class="section-heading">
              <div>
                <h2 id="entitlement-title">
                  用量与权益
                </h2><p v-if="entitlements?.enabled">
                  {{ entitlements.source === 'personal' ? '个人专属配置' : '继承用户组或默认配置' }}<span v-if="entitlements.unlimited"> · 无额度上限</span>
                </p>
              </div>
              <button
                class="icon-action"
                title="刷新额度"
                aria-label="刷新额度"
                :disabled="quotaLoading"
                @click="loadQuotas"
              >
                <RefreshCw
                  :size="16"
                  :class="{ 'animate-spin': quotaLoading }"
                />
              </button>
            </div>
            <div
              v-if="quotaError"
              role="alert"
              class="load-error"
            >
              额度暂不可用，不代表没有额度。<button @click="loadQuotas">
                重试
              </button>
            </div>
            <p
              v-else-if="quotaLoading && !entitlements"
              class="loading-text"
              role="status"
            >
              正在读取额度…
            </p>
            <template v-else-if="entitlements?.enabled">
              <div class="quota-list">
                <QuotaWindow
                  v-for="window in totalWindows"
                  :key="window.period"
                  :window="window"
                  :timezone="entitlements.timezone"
                />
              </div>
              <p class="text-xs text-muted-foreground mt-2">
                已结算及处理中占用共同计入额度。更新时间 {{ updatedAt }}
              </p>
              <div
                v-if="subpools.length"
                class="subpools"
              >
                <h3 class="font-medium text-sm">
                  模型与家族子池
                </h3>
                <p class="text-xs text-muted-foreground mt-1">
                  子池用量同时计入总额度；未单独设置的模型仅受总额度限制。
                </p>
                <details
                  v-for="pool in subpools"
                  :key="pool.id"
                  class="model-pool"
                >
                  <summary><span class="break-all">{{ pool.label }}</span><span class="text-xs text-muted-foreground shrink-0">{{ pool.windows.length }} 项限额</span></summary>
                  <p
                    v-if="pool.members.length"
                    class="text-xs text-muted-foreground break-all mt-2"
                  >
                    {{ pool.members.join('、') }}
                  </p>
                  <QuotaWindow
                    v-for="window in pool.windows"
                    :key="window.period"
                    :window="window"
                    :timezone="entitlements.timezone"
                  />
                </details>
              </div>
            </template>
            <p
              v-else
              class="loading-text"
            >
              当前未启用内部权益。
            </p>
          </section>

          <TokenActivity class="profile-section" />

          <section
            class="profile-section"
            aria-labelledby="usage-title"
          >
            <div class="section-heading">
              <h2 id="usage-title">
                调用概览
              </h2>
              <div class="flex gap-2 items-center">
                <select
                  v-model="preset"
                  aria-label="个人用量统计区间"
                  class="period-select"
                >
                  <option value="today">
                    今天
                  </option><option value="last7days">
                    最近 7 天
                  </option><option value="this_month">
                    本月
                  </option>
                </select>
                <button
                  class="icon-action"
                  title="刷新调用统计"
                  aria-label="刷新调用统计"
                  :disabled="usageLoading"
                  @click="loadUsage"
                >
                  <RefreshCw
                    :size="16"
                    :class="{ 'animate-spin': usageLoading }"
                  />
                </button>
              </div>
            </div>
            <div
              v-if="usageError"
              role="alert"
              class="load-error"
            >
              调用统计加载失败。<button @click="loadUsage">
                重试
              </button>
            </div>
            <p
              v-else-if="usageLoading"
              class="loading-text"
              role="status"
            >
              正在加载调用统计…
            </p>
            <template v-else-if="usage">
              <dl class="usage-metrics">
                <div><dt>请求次数</dt><dd>{{ number(usage.total_requests) }}</dd></div><div><dt>Token 用量</dt><dd>{{ number(usage.total_tokens) }}</dd></div><div><dt>用量费用</dt><dd>{{ money(usage.total_cost) }}</dd></div>
              </dl>
              <p class="text-xs text-muted-foreground mb-5">
                用量费用按模型标准费率统计；额度占用以上方权益账本为准。
              </p>
              <div
                v-if="usage.summary_by_model.length"
                class="model-usage"
              >
                <div class="table-scroll">
                  <table>
                    <caption class="sr-only">
                      个人模型用量
                    </caption><thead><tr><th>模型</th><th>请求</th><th>Token</th><th>费用</th></tr></thead><tbody>
                      <tr
                        v-for="model in usage.summary_by_model"
                        :key="model.model"
                      >
                        <td>{{ model.model }}</td><td>{{ number(model.requests) }}</td><td>{{ number(model.total_tokens) }}</td><td>{{ money(model.total_cost_usd) }}</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
              <p
                v-else
                class="loading-text"
              >
                此区间内暂无调用记录。
              </p>
              <div
                v-if="usage.records.length"
                class="recent-usage"
              >
                <h3 class="text-sm font-medium mb-2">
                  最近调用
                </h3>
                <div
                  v-for="record in usage.records"
                  :key="record.id"
                  class="recent-row"
                >
                  <span class="truncate min-w-0">{{ record.model }}</span><time>{{ date(record.created_at) }}</time><span :class="record.status_code >= 400 ? 'text-destructive' : 'text-muted-foreground'">{{ record.status_code >= 400 ? '失败' : record.status_code >= 200 ? '成功' : '处理中' }}</span>
                </div>
              </div>
              <RouterLink
                to="/dashboard/usage"
                class="text-link mt-4 inline-flex gap-1 items-center"
              >
                查看完整使用记录<ArrowUpRight :size="14" />
              </RouterLink>
            </template>
          </section>

          <section
            class="profile-section"
            aria-labelledby="models-title"
          >
            <div class="section-heading">
              <h2 id="models-title">
                可用模型
              </h2><RouterLink
                to="/dashboard/models"
                class="text-link"
              >
                查看模型目录
              </RouterLink>
            </div>
            <p
              v-if="modelsError"
              role="alert"
              class="load-error"
            >
              模型列表暂不可用。<button @click="loadModels">
                重试
              </button>
            </p>
            <p
              v-else-if="!models"
              class="loading-text"
            >
              正在加载模型…
            </p>
            <template v-else>
              <p class="text-sm text-muted-foreground">
                {{ models.total ? `当前可使用 ${models.total} 个模型` : '暂未分配可用模型，请联系管理员。' }}
              </p><ul class="available-models">
                <li
                  v-for="model in models.models"
                  :key="model.id"
                >
                  {{ model.display_name || model.name }}
                </li>
              </ul>
            </template>
          </section>
        </template>
        <template v-else>
          <div class="section-heading">
            <h2>{{ sections.find(s => s.id === section)?.label }}</h2>
          </div>
        </template>
        <KeepAlive>
          <Settings
            v-if="section !== 'usage'"
            embedded
            :section="section"
            @updated="loadProfile"
          />
        </KeepAlive>
        <dl
          v-if="section === 'account' && profile"
          class="account-meta"
        >
          <div><dt>账户类型</dt><dd>{{ roles[profile.role] || '普通用户' }}</dd></div><div><dt>注册时间</dt><dd>{{ date(profile.created_at) }}</dd></div><div><dt>最近登录</dt><dd>{{ profile.last_login_at ? date(profile.last_login_at) : '未记录' }}</dd></div>
        </dl>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { RouterLink } from 'vue-router'
import { ArrowUpRight, ChartNoAxesCombined, KeyRound, RefreshCw, Settings2, ShieldCheck, UserRound } from 'lucide-vue-next'
import { accountApi, type AccountEntitlements } from '@/api/account'
import { meApi, type Profile, type UsageResponse } from '@/api/me'
import UserAvatar from '@/components/common/UserAvatar.vue'
import { useAuthStore } from '@/stores/auth'
import Settings from './Settings.vue'
import QuotaWindow from './profile/QuotaWindow.vue'
import TokenActivity from './profile/TokenActivity.vue'

type Section = 'usage' | 'account' | 'security' | 'preferences'
const sections = [{ id: 'usage' as const, label: '用量与权益', icon: ChartNoAxesCombined }, { id: 'account' as const, label: '个人信息', icon: UserRound }, { id: 'security' as const, label: '安全', icon: ShieldCheck }, { id: 'preferences' as const, label: '偏好设置', icon: Settings2 }]
const section = ref<Section>('usage')
const auth = useAuthStore()
const profile = ref<Profile>(), entitlements = ref<AccountEntitlements>(), usage = ref<UsageResponse>(), models = ref<Awaited<ReturnType<typeof meApi.getAvailableModels>>>()
const profileError = ref(false), quotaError = ref(false), usageError = ref(false), modelsError = ref(false)
const quotaLoading = ref(false), usageLoading = ref(false), preset = ref('this_month')
let usageVersion = 0, quotaVersion = 0, alive = true
let quotaAbort: AbortController | undefined
const roles: Record<string, string> = { admin: '超级管理员', audit_admin: '审计管理员', user_admin: '用户及权益管理员', service_admin: '服务管理员', user: '普通用户' }
const order = { five_hour: 0, daily: 1, weekly: 2, monthly: 3 }
const totalWindows = computed(() => entitlements.value?.enabled ? entitlements.value.windows.filter(w => !w.model && !w.family).sort((a, b) => order[a.period] - order[b.period]) : [])
const subpools = computed(() => {
  if (!entitlements.value?.enabled) return []
  const windows = entitlements.value.windows
  const key = (w: typeof windows[number]) => w.family ? `family:${w.family}` : w.model ? `model:${w.model}` : ''
  return [...new Set(windows.map(key).filter(Boolean))].flatMap(id => {
    const pool = windows.filter(w => key(w) === id).sort((a, b) => order[a.period] - order[b.period])
    const first = pool[0]
    return first ? [{ id, label: first.family ? (first.family_name || first.family) : first.model || '', members: first.family ? first.models || [] : [], windows: pool }] : []
  })
})
const updatedAt = computed(() => entitlements.value?.enabled ? new Date(entitlements.value.generated_at * 1000).toLocaleTimeString('zh-CN', { hour12: false }) : '')
const number = (value: number) => new Intl.NumberFormat('zh-CN').format(value)
const money = (value: number) => new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 4 }).format(value)
function date(value: string) { const parsed = new Date(value); return Number.isFinite(parsed.valueOf()) ? parsed.toLocaleString('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false }) : '未记录' }
async function loadProfile() { profileError.value = false; try { const data = await meApi.getProfile(); if (alive) profile.value = data } catch { if (alive) profileError.value = true } }
async function loadQuotas() {
  const version = ++quotaVersion
  quotaAbort?.abort(); quotaAbort = new AbortController()
  quotaLoading.value = true; quotaError.value = false
  try { const data = await accountApi.entitlements(quotaAbort.signal); if (alive && version === quotaVersion) entitlements.value = data }
  catch { if (alive && version === quotaVersion) quotaError.value = true }
  finally { if (alive && version === quotaVersion) quotaLoading.value = false }
}
async function loadUsage() {
  const version = ++usageVersion
  usageLoading.value = true; usageError.value = false
  try {
    const data = await meApi.getUsage({ preset: preset.value, tz_offset_minutes: -new Date().getTimezoneOffset(), limit: 5, offset: 0 })
    if (alive && version === usageVersion) usage.value = data
  } catch { if (alive && version === usageVersion) usageError.value = true }
  finally { if (alive && version === usageVersion) usageLoading.value = false }
}
async function loadModels() { modelsError.value = false; try { const data = await meApi.getAvailableModels({ limit: 8, skip: 0 }); if (alive) models.value = data } catch { if (alive) modelsError.value = true } }
watch(preset, loadUsage)
onMounted(() => { void loadProfile(); void loadQuotas(); void loadUsage(); void loadModels() })
onUnmounted(() => { alive = false; quotaAbort?.abort(); usageVersion++; quotaVersion++ })
</script>

<style scoped>
.profile-page { max-width: 1060px; margin: 0 auto; padding: 2rem 2rem 4rem; color: var(--foreground); letter-spacing: 0; }
.profile-heading { display: flex; align-items: center; gap: 1rem; padding-bottom: 2rem; border-bottom: 1px solid var(--border); }
.profile-avatar { width: 48px; height: 48px; flex-shrink: 0; border-radius: 50%; display: grid; place-items: center; background: var(--muted); font-size: 16px; font-weight: 600; }
.profile-status { font-size: 12px; color: var(--muted-foreground); white-space: nowrap; }
.profile-layout { display: grid; grid-template-columns: 176px minmax(0, 1fr); gap: 3rem; padding-top: 2rem; }
.profile-nav { display: flex; flex-direction: column; gap: 4px; align-self: start; }
.profile-nav button,.profile-nav a { display: flex; align-items: center; gap: .65rem; min-height: 40px; padding: .65rem .75rem; border-radius: 6px; font-size: 13px; text-align: left; color: var(--muted-foreground); }
.profile-nav button:hover,.profile-nav a:hover,.profile-nav [aria-current="page"] { color: var(--foreground); background: var(--muted); }
.profile-nav [aria-current="page"] { font-weight: 500; }
.profile-content { min-width: 0; }
.section-heading { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-bottom: .75rem; }
.section-heading h2 { font-size: 16px; font-weight: 600; }
.section-heading p { font-size: 12px; color: var(--muted-foreground); margin-top: .4rem; }
.icon-action { width: 32px; height: 32px; display: grid; place-items: center; border-radius: 6px; flex-shrink: 0; }
.icon-action:hover { background: var(--muted); }.icon-action:disabled { opacity: .5; }
.profile-section { margin-top: 2rem; padding-top: 1.75rem; border-top: 1px solid var(--border); }
.quota-list > * + * { border-top: 1px solid color-mix(in srgb, var(--border) calc(.5 * 100%), transparent); }
.subpools { margin-top: 1.75rem; }.model-pool { border-bottom: 1px solid var(--border); margin-top: .5rem; }
.model-pool summary { display: flex; justify-content: space-between; align-items: center; gap: 1rem; padding: .85rem 0; cursor: pointer; font-size: 13px; }
.model-pool summary::before { content: '+'; color: var(--muted-foreground); }.model-pool[open] summary::before { content: '-'; }.model-pool summary span:first-child { flex: 1; }
.usage-metrics { display: grid; grid-template-columns: repeat(3,minmax(0,1fr)); gap: 1rem; padding: .75rem 0 1.25rem; }.usage-metrics dt { font-size: 12px; color: var(--muted-foreground); }.usage-metrics dd { font-size: 22px; font-weight: 500; margin-top: .4rem; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }
.period-select { background: var(--background); border: 1px solid var(--border); border-radius: 6px; padding: .4rem .5rem; font-size: 12px; min-height: 32px; }
.table-scroll { overflow-x: auto; }table { width: 100%; font-size: 12px; }th,td { padding: .7rem .4rem; text-align: right; border-bottom: 1px solid color-mix(in srgb, var(--border) calc(.6 * 100%), transparent); white-space: nowrap; font-variant-numeric: tabular-nums; }th { color: var(--muted-foreground); font-weight: 400; }th:first-child,td:first-child { text-align: left; padding-left: 0; max-width: 230px; overflow-wrap: anywhere; white-space: normal; }
.recent-usage { margin-top: 1.5rem; }.recent-row { display: grid; grid-template-columns: minmax(0,1fr) auto 32px; gap: 1rem; padding: .6rem 0; font-size: 12px; }.recent-row time { color: var(--muted-foreground); }
.available-models { display: flex; flex-wrap: wrap; gap: .5rem 1.5rem; margin-top: .75rem; font-size: 13px; }.available-models li { overflow-wrap: anywhere; }
.text-link { font-size: 12px; color: var(--muted-foreground); }.text-link:hover { color: var(--foreground); text-decoration: underline; }
.load-error { padding: 1rem 0; font-size: 13px; color: var(--destructive); }.load-error button { text-decoration: underline; margin-left: .5rem; }.loading-text { padding: 1rem 0; font-size: 13px; color: var(--muted-foreground); }
.account-meta { font-size: 13px; margin-top: 1.5rem; }.account-meta div { display: flex; justify-content: space-between; gap: 1rem; padding: .6rem 0; }.account-meta dt { color: var(--muted-foreground); }
@media(max-width: 900px) { .profile-layout { gap: 1.5rem; grid-template-columns: 155px minmax(0,1fr); }.profile-page { padding: 1.5rem; } }
@media(max-width: 640px) { .profile-page { padding: 1.25rem 1rem 3rem; }.profile-heading { gap: .75rem; padding-bottom: 1.25rem; }.profile-status { display: none; }.profile-layout { display: block; padding-top: 1rem; }.profile-nav { flex-direction: row; overflow-x: auto; margin: 0 -1rem 1.5rem; padding: 0 1rem .5rem; }.profile-nav button,.profile-nav a { white-space: nowrap; flex-shrink: 0; }.profile-nav a { display: none; }.profile-content { width: 100%; }.usage-metrics { gap: .6rem; }.usage-metrics dd { font-size: 18px; }.section-heading { gap: .5rem; }.recent-row { gap: .5rem; } }
</style>
