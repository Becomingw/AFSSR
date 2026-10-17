<template>
  <div class="benefits-page min-w-0 pb-6">
    <PageHeader title="用户权益">
      <template #actions>
        <span
          class="benefits-save-state"
          role="status"
        ><LockKeyhole
          v-if="!canWrite"
          class="h-3.5 w-3.5"
        />{{ !canWrite ? '只读' : loading ? '加载中' : hasChanges ? '未保存' : saved ? '已保存' : '已保存的配置' }}</span>
        <Button
          variant="outline"
          size="icon"
          title="刷新"
          :disabled="loading"
          @click="requestReload"
        >
          <RefreshCw
            class="h-4 w-4"
            :class="{ 'animate-spin': loading }"
          />
        </Button>
        <Button
          :disabled="busy"
          @click="save()"
        >
          <Save class="h-4 w-4" />保存更改
        </Button>
      </template>
    </PageHeader>
    <p
      v-if="errorMessage"
      role="alert"
      class="benefits-alert"
    >
      <AlertCircle class="h-4 w-4 shrink-0" />{{ errorMessage }}
    </p>
    <nav
      class="benefits-tabs"
      role="tablist"
      aria-label="权益视图"
      @keydown="moveTab($event, 'page')"
    >
      <button
        v-for="tab in views"
        :id="`benefits-tab-${tab.id}`"
        :key="tab.id"
        type="button"
        role="tab"
        :aria-selected="view === tab.id"
        :aria-controls="`benefits-panel-${tab.id}`"
        :tabindex="view === tab.id ? 0 : -1"
        @click="view = tab.id"
      >
        <component
          :is="tab.icon"
          class="h-4 w-4"
        />{{ tab.label }}
      </button>
    </nav>
    <div
      v-show="view === 'allocation'"
      id="benefits-panel-allocation"
      role="tabpanel"
      aria-labelledby="benefits-tab-allocation"
      :aria-hidden="view !== 'allocation'"
      class="benefits-workspace"
    >
      <aside
        class="benefits-subjects"
        aria-label="分配对象"
      >
        <div
          class="benefits-segments"
          role="radiogroup"
          aria-label="分配对象类型"
          @keydown="moveTab($event, 'target')"
        >
          <button
            v-for="mode in targetModes"
            :id="`benefits-target-${mode.id}`"
            :key="mode.id"
            type="button"
            role="radio"
            :aria-checked="targetType === mode.id"
            :tabindex="targetType === mode.id ? 0 : -1"
            :disabled="loading"
            @click="changeTargetType(mode.id)"
          >
            {{ mode.label }}
          </button>
        </div>
        <label
          v-if="targetType !== 'default'"
          class="benefits-search"
        ><Search class="h-4 w-4" /><Input
          v-model="subjectSearch"
          aria-label="搜索分配对象"
          placeholder="搜索分配对象"
          :disabled="loading"
        /></label>
        <div
          class="benefits-subject-list"
          role="group"
          aria-label="分配对象列表"
        >
          <button
            v-for="option in filteredOptions"
            :key="option.id"
            type="button"
            :aria-pressed="targetId === option.id"
            :disabled="loading"
            class="benefits-subject"
            @click="targetId = option.id; loadDraft()"
          >
            <span class="min-w-0 truncate">{{ option.name }}</span><span
              v-if="subjectDirty(option.id)"
              class="benefits-dirty-dot"
              aria-label="未保存"
            />
          </button>
          <p
            v-if="!filteredOptions.length"
            class="benefits-empty"
          >
            {{ loading ? '加载中' : '无匹配对象' }}
          </p>
        </div>
      </aside>
      <section
        class="benefits-editor"
        aria-label="权益分配"
      >
        <template v-if="targetId">
          <header class="benefits-editor-heading">
            <div class="min-w-0">
              <h2 class="break-words">
                {{ selectedName }}
              </h2><span class="benefits-source">{{ targetType === 'default' ? '默认权益' : targetType === 'group' ? '用户组' : '个人权益' }}<span class="mx-2">/</span>{{ allocationSource }}</span>
            </div>
            <div class="flex flex-wrap items-center gap-3">
              <Button
                v-if="targetType === 'user'"
                variant="ghost"
                size="sm"
                :disabled="busy"
                @click="removeOverride"
              >
                <Undo2 class="h-4 w-4" />恢复组权益
              </Button>
              <label class="benefits-unlimited"><span>不限额</span><Switch
                v-model="draft.unlimited"
                aria-label="不限额"
                :disabled="allocationBusy"
              /></label>
            </div>
          </header>
          <div
            v-if="editor.removed"
            role="status"
            class="benefits-pending"
          >
            <span>待移除个人覆盖</span><Button
              variant="outline"
              size="sm"
              :disabled="busy"
              @click="editor.removed = false"
            >
              <Undo2 class="h-4 w-4" />保留个人覆盖
            </Button>
          </div>
          <div class="min-w-0">
            <template v-if="!draft.unlimited">
              <section
                class="benefits-total"
                aria-label="总额度"
              >
                <h3>总额度 <span>USD</span></h3>
                <div class="benefits-budget-grid">
                  <label
                    v-for="budget in totalBudgets"
                    :key="budget.key"
                    class="benefits-money-field"
                  ><span>{{ budget.shortLabel }}</span><div class="benefits-money-input"><Input
                    type="number"
                    min="0"
                    step="0.01"
                    :aria-label="budget.label"
                    :model-value="draft[budget.key] ?? ''"
                    :disabled="allocationBusy"
                    placeholder="不限"
                    @update:model-value="draft[budget.key] = optionalNumber($event)"
                  /><span>USD</span></div></label>
                </div>
              </section>
              <div class="benefits-pool-heading">
                <nav
                  class="benefits-tabs benefits-tabs--secondary"
                  role="tablist"
                  aria-label="子池类型"
                  @keydown="moveTab($event, 'pool')"
                >
                  <button
                    id="benefits-pool-family"
                    type="button"
                    role="tab"
                    :aria-selected="poolView === 'family'"
                    aria-controls="benefits-family-pool"
                    :tabindex="poolView === 'family' ? 0 : -1"
                    @click="poolView = 'family'"
                  >
                    家族共享池 <span>{{ familyRows.length }}</span>
                  </button>
                  <button
                    id="benefits-pool-model"
                    type="button"
                    role="tab"
                    :aria-selected="poolView === 'model'"
                    aria-controls="benefits-model-pool"
                    :tabindex="poolView === 'model' ? 0 : -1"
                    @click="poolView = 'model'"
                  >
                    单模型池 <span>{{ rows.length }}</span>
                  </button>
                </nav>
                <Button
                  v-show="poolView === 'family'"
                  variant="ghost"
                  size="sm"
                  :disabled="allocationBusy || rows.length + familyRows.length >= 15 || !Object.keys(families).length"
                  @click="familyRows.push({ family: '', quota: {} })"
                >
                  <Plus class="h-4 w-4" />添加家族子池
                </Button>
                <Button
                  v-show="poolView === 'model'"
                  variant="ghost"
                  size="sm"
                  :disabled="allocationBusy || rows.length + familyRows.length >= 15"
                  @click="rows.push({ model: '', quota: {} })"
                >
                  <Plus class="h-4 w-4" />添加子池
                </Button>
              </div>
              <div
                v-show="poolView === 'family'"
                id="benefits-family-pool"
                role="tabpanel"
                aria-labelledby="benefits-pool-family"
                :aria-hidden="poolView !== 'family'"
                class="benefits-pool-scroll"
              >
                <table
                  class="console-quota-table benefits-quota-table"
                  aria-label="家族共享子池额度"
                >
                  <thead>
                    <tr>
                      <th>模型家族</th><th
                        v-for="window in windows"
                        :key="window.key"
                      >
                        {{ window.shortLabel }}
                      </th><th class="w-10">
                        <span class="sr-only">操作</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-if="!familyRows.length">
                      <td
                        colspan="6"
                        class="benefits-pool-empty"
                      >
                        无家族子池
                      </td>
                    </tr>
                    <tr
                      v-for="(row, index) in familyRows"
                      :key="index"
                    >
                      <td data-label="模型家族">
                        <select
                          v-model="row.family"
                          aria-label="限额家族"
                          :disabled="allocationBusy"
                        >
                          <option
                            value=""
                            disabled
                          >
                            请选择家族
                          </option><option
                            v-if="row.family && !families[row.family]"
                            :value="row.family"
                          >
                            {{ row.family }}（不存在）
                          </option><option
                            v-for="(family, id) in families"
                            :key="id"
                            :value="id"
                            :disabled="familyRows.some((other, otherIndex) => otherIndex !== index && other.family === id)"
                          >
                            {{ family.name || id }}
                          </option>
                        </select>
                      </td>
                      <td
                        v-for="window in windows"
                        :key="window.key"
                        :data-label="window.label"
                      >
                        <Input
                          type="number"
                          min="0"
                          step="0.01"
                          :aria-label="`${families[row.family]?.name || '未选择家族'} ${window.label}额度`"
                          :disabled="allocationBusy"
                          :model-value="row.quota[window.key] ?? ''"
                          placeholder="不限"
                          @update:model-value="row.quota[window.key] = optionalNumber($event)"
                        />
                      </td>
                      <td>
                        <Button
                          variant="ghost"
                          size="icon"
                          title="移除家族子池"
                          :disabled="allocationBusy"
                          @click="familyRows.splice(index, 1)"
                        >
                          <Trash2 class="h-4 w-4" />
                        </Button>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
              <div
                v-show="poolView === 'model'"
                id="benefits-model-pool"
                role="tabpanel"
                aria-labelledby="benefits-pool-model"
                :aria-hidden="poolView !== 'model'"
                class="benefits-pool-scroll"
              >
                <table
                  class="console-quota-table benefits-quota-table"
                  aria-label="模型子池额度"
                >
                  <thead>
                    <tr>
                      <th>单独限额模型</th><th
                        v-for="window in windows"
                        :key="window.key"
                      >
                        {{ window.shortLabel }}
                      </th><th class="w-10">
                        <span class="sr-only">操作</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-if="!rows.length">
                      <td
                        colspan="6"
                        class="benefits-pool-empty"
                      >
                        无单独子池
                      </td>
                    </tr>
                    <tr
                      v-for="(row, index) in rows"
                      :key="index"
                    >
                      <td data-label="单独限额模型">
                        <Input
                          v-model="row.model"
                          :disabled="allocationBusy"
                          aria-label="单独限额模型"
                          placeholder="模型名称或 *"
                        />
                      </td>
                      <td
                        v-for="window in windows"
                        :key="window.key"
                        :data-label="window.label"
                      >
                        <Input
                          type="number"
                          min="0"
                          step="0.01"
                          :aria-label="`${row.model || '未命名模型'} ${window.label}额度`"
                          :disabled="allocationBusy"
                          :model-value="row.quota[window.key] ?? ''"
                          placeholder="不限"
                          @update:model-value="row.quota[window.key] = optionalNumber($event)"
                        />
                      </td>
                      <td>
                        <Button
                          variant="ghost"
                          size="icon"
                          title="移除模型子池"
                          :disabled="allocationBusy"
                          @click="rows.splice(index, 1)"
                        >
                          <Trash2 class="h-4 w-4" />
                        </Button>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </template>
            <p
              v-else
              class="benefits-unlimited-state"
            >
              <ShieldCheck class="h-5 w-5" />不限额
            </p>
          </div>
          <div class="benefits-timezone">
            <Globe2 class="h-4 w-4" /><label for="benefits-timezone">重置时区</label><Input
              id="benefits-timezone"
              v-model="config.timezone"
              aria-label="重置时区"
              :disabled="busy"
            />
          </div>
        </template>
        <div
          v-else
          class="benefits-unselected"
        >
          <UsersRound class="h-7 w-7" /><h2>未选择分配对象</h2>
        </div>
      </section>
    </div>
    <div
      v-show="view === 'families'"
      id="benefits-panel-families"
      role="tabpanel"
      aria-labelledby="benefits-tab-families"
      :aria-hidden="view !== 'families'"
      class="benefits-family-view"
    >
      <ModelFamilyEditor
        v-model="families"
        :models="catalogModels"
        :references="familyReferences"
        :disabled="busy"
        :catalog-error="catalogError"
      />
    </div>
    <div
      v-show="view === 'preview'"
      id="benefits-panel-preview"
      role="tabpanel"
      aria-labelledby="benefits-tab-preview"
      :aria-hidden="view !== 'preview'"
      class="benefits-preview-view"
    >
      <EntitlementPreview
        :users="users"
        :get-draft="buildDraft"
        :selected-user="targetType === 'user' ? targetId : undefined"
        :disabled="loading || !loaded"
      />
    </div>
    <Dialog
      v-model="refreshConfirmation"
      title="放弃未保存的更改？"
      size="sm"
    >
      <p class="text-sm text-muted-foreground">
        当前草稿尚未保存。
      </p>
      <template #footer>
        <Button
          variant="destructive"
          @click="refreshConfirmation = false; reload()"
        >
          放弃更改并刷新
        </Button><Button
          variant="outline"
          @click="refreshConfirmation = false"
        >
          继续编辑
        </Button>
      </template>
    </Dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { AlertCircle, Eye, Globe2, Layers3, LockKeyhole, Plus, RefreshCw, Save, Search, Settings2, ShieldCheck, Trash2, Undo2, UsersRound } from 'lucide-vue-next'
import { Button, Dialog, Input, Switch } from '@/components/ui'
import './internal-access.css'
import PageHeader from '@/components/layout/PageHeader.vue'
import ModelFamilyEditor from './components/ModelFamilyEditor.vue'
import EntitlementPreview from './components/EntitlementPreview.vue'
import { usersApi } from '@/api/users'
import { getGlobalModels as apiGlobalModels } from '@/api/global-models'
import { emptyAllocation, emptyConfig, loadInternalAccess, saveInternalAccess, type Allocation, type InternalAccessConfig, type ModelAllowance, type ModelFamily } from '@/api/internal-access'
import { parseApiError } from '@/utils/errorParser'
import { useAuthStore } from '@/stores/auth'

const config = ref(emptyConfig())
const view = ref<'allocation' | 'families' | 'preview'>('allocation')
const views = [{ id: 'allocation' as const, label: '额度分配', icon: Settings2 }, { id: 'families' as const, label: '模型家族', icon: Layers3 }, { id: 'preview' as const, label: '生效预览', icon: Eye }]
const poolView = ref<'family' | 'model'>('family')
const targetModes = [{ id: 'default' as const, label: '默认' }, { id: 'group' as const, label: '用户组' }, { id: 'user' as const, label: '个人' }]
const subjectSearch = ref(''), savedTimezone = ref(''), refreshConfirmation = ref(false)
interface AllocationEditor {
  allocation: Allocation
  models: { model: string; quota: ModelAllowance }[]
  families: { family: string; quota: ModelAllowance }[]
  removed?: boolean
  initial: string
}
const editors = ref<Record<string, AllocationEditor>>({})
const families = ref<Record<string, ModelFamily>>({})
const catalogModels = ref<string[]>([])
const catalogError = ref('')
const targetType = ref<'default' | 'group' | 'user'>('default')
const targetId = ref('default')
const editorKey = computed(() => `${targetType.value}:${targetId.value}`)
function createEditor(source?: Allocation | null): AllocationEditor {
  const allocation = source ? clone(source) : emptyAllocation()
  const edited = { allocation, models: Object.entries(allocation.models ?? {}).map(([model, quota]) => ({ model, quota })), families: Object.entries(allocation.families ?? {}).map(([family, quota]) => ({ family, quota })) }
  return { ...edited, initial: JSON.stringify(edited) }
}
const fallbackEditor = createEditor()
const editor = computed(() => editors.value[editorKey.value] ?? fallbackEditor)
watch([editorKey, () => editors.value], () => {
  const key = editorKey.value
  if (!editors.value[key]) {
    const source = targetType.value === 'default' ? config.value.default : targetType.value === 'group'
      ? config.value.groups[targetId.value] : config.value.users[targetId.value]
    editors.value[key] = createEditor(source)
  }
}, { immediate: true, flush: 'sync' })
const draft = computed(() => editor.value.allocation)
const rows = computed(() => editor.value.models)
const familyRows = computed(() => editor.value.families)
const groups = ref<{ id: string; name: string }[]>([])
const users = ref<{ id: string; name: string }[]>([])
const options = computed(() => targetType.value === 'default' ? [{ id: 'default', name: '默认权益' }] : targetType.value === 'group' ? groups.value : users.value)
const filteredOptions = computed(() => options.value.filter(option => option.name.toLocaleLowerCase().includes(subjectSearch.value.trim().toLocaleLowerCase())))
const selectedName = computed(() => options.value.find(option => option.id === targetId.value)?.name || targetId.value)
const allocationSource = computed(() => targetType.value === 'default' ? '基础分配' : targetType.value === 'group' ? (config.value.groups[targetId.value] || subjectDirty(targetId.value) ? '组权益' : '未单独配置') : (config.value.users[targetId.value] || subjectDirty(targetId.value) ? '个人覆盖' : '继承权益'))
const hasChanges = computed(() => config.value.timezone !== savedTimezone.value || JSON.stringify(families.value) !== JSON.stringify(config.value.model_families ?? {}) || Object.values(editors.value).some(isDirty))
const authStore = useAuthStore()
const canWrite = computed(() => authStore.canManageUsers)
const loading = ref(false)
const loaded = ref(false)
const busy = computed(() => loading.value || !loaded.value || !canWrite.value)
const allocationBusy = computed(() => busy.value || !targetId.value || !!editor.value.removed)
const saved = ref(false)
const errorMessage = ref('')
const totalBudgets: { key: 'five_hour_budget_usd' | 'weekly_budget_usd' | 'monthly_budget_usd'; label: string; shortLabel: string }[] = [
  { key: 'five_hour_budget_usd', label: '滚动 5 小时总金额上限（USD）', shortLabel: '滚动 5 小时' },
  { key: 'weekly_budget_usd', label: '每周总金额上限（USD）', shortLabel: '每周' },
  { key: 'monthly_budget_usd', label: '每月总金额上限（USD）', shortLabel: '每月' },
]
const windows: { key: keyof ModelAllowance; label: string; shortLabel: string }[] = [
  { key: 'five_hour_budget_usd', label: '滚动 5 小时', shortLabel: '5 小时' }, { key: 'daily_budget_usd', label: '每日', shortLabel: '每日' },
  { key: 'weekly_budget_usd', label: '每周（周一）', shortLabel: '每周' }, { key: 'monthly_budget_usd', label: '每月', shortLabel: '每月' },
]
function optionalNumber(value: string | number): number | undefined {
  return value === '' ? undefined : Number(value)
}
function selectTarget() {
  targetId.value = targetType.value === 'default' ? 'default' : ''
  loadDraft()
}
function changeTargetType(type: 'default' | 'group' | 'user') { targetType.value = type; subjectSearch.value = ''; selectTarget() }
function isDirty(edited: AllocationEditor) { return !!edited.removed || edited.initial !== JSON.stringify({ allocation: edited.allocation, models: edited.models, families: edited.families }) }
function subjectDirty(id: string) { const edited = editors.value[`${targetType.value}:${id}`]; return edited ? isDirty(edited) : false }
function requestReload() { if (hasChanges.value && loaded.value) refreshConfirmation.value = true; else void reload() }
function moveTab(event: KeyboardEvent, kind: 'page' | 'target' | 'pool') {
  if (!['ArrowRight', 'ArrowLeft', 'Home', 'End'].includes(event.key)) return
  const ids = kind === 'page' ? views.map(tab => tab.id) : kind === 'target' ? targetModes.map(mode => mode.id) : ['family', 'model']
  const current = kind === 'page' ? view.value : kind === 'target' ? targetType.value : poolView.value
  const index = event.key === 'Home' ? 0 : event.key === 'End' ? ids.length - 1 : (ids.indexOf(current) + (event.key === 'ArrowRight' ? 1 : -1) + ids.length) % ids.length
  const id = ids[index]
  if (!id) return
  event.preventDefault()
  if (kind === 'page') view.value = id as typeof view.value
  else if (kind === 'target') { if (loading.value) return; changeTargetType(id as typeof targetType.value) }
  else poolView.value = id as typeof poolView.value
  document.getElementById(`benefits-${kind === 'page' ? 'tab' : kind}-${id}`)?.focus()
}
function loadDraft() {
  saved.value = false
}
function clone<T>(value: T): T { return JSON.parse(JSON.stringify(value)) }

// Reference checks include untouched allocations and raw unsaved rows, even in unlimited mode.
const familyReferences = computed(() => {
  const refs: Record<string, string[]> = Object.create(null)
  function add(key: string, allocation: Allocation | null | undefined, label: string) {
    const edited = editors.value[key]
    if (edited?.removed) return
    const ids = edited ? edited.families.map(row => row.family) : Object.keys(allocation?.families ?? {})
    for (const id of new Set(ids.filter(Boolean))) (refs[id] ??= []).push(label)
  }
  add('default:default', config.value.default, '默认权益')
  const groupIds = new Set([...Object.keys(config.value.groups), ...Object.keys(editors.value).filter(key => key.startsWith('group:')).map(key => key.slice(6))])
  const userIds = new Set([...Object.keys(config.value.users), ...Object.keys(editors.value).filter(key => key.startsWith('user:')).map(key => key.slice(5))])
  for (const id of groupIds) if (id) add(`group:${id}`, config.value.groups[id], `组 ${groups.value.find(group => group.id === id)?.name || id}`)
  for (const id of userIds) if (id) add(`user:${id}`, config.value.users[id], `用户 ${users.value.find(user => user.id === id)?.name || id}`)
  return refs
})
async function loadCatalog() {
  catalogError.value = ''
  try {
    const names = new Set<string>()
    let skip = 0
    while (true) {
      const page = await apiGlobalModels({ skip, limit: 1000 })
      for (const model of page.models) names.add(model.name)
      skip += page.models.length
      if (skip >= page.total) break
      if (!page.models.length) throw new Error('模型目录分页不完整')
    }
    catalogModels.value = [...names].sort()
  } catch (error) { catalogError.value = parseApiError(error, '加载公开模型目录失败') }
}
async function reload() {
  loading.value = true
  loaded.value = false
  errorMessage.value = ''
  try {
    config.value = await loadInternalAccess()
    savedTimezone.value = config.value.timezone
    families.value = clone(config.value.model_families ?? {})
    editors.value = {}
    groups.value = (await usersApi.listUserGroups()).items.map(g => ({ id: g.id, name: g.name }))
    const result = await usersApi.getAllUsersPage({ limit: 1000 })
    users.value = result.items.map(u => ({ id: u.id, name: u.username }))
    await loadCatalog()
    loaded.value = true
    loadDraft()
  } catch (error) { errorMessage.value = parseApiError(error, '加载权益失败') }
  finally { loading.value = false }
}
async function save(remove = false) {
  if (busy.value) return
  if (remove && targetId.value) editor.value.removed = true
  loading.value = true
  errorMessage.value = ''
  try {
    const next = buildDraft()
    await saveInternalAccess(next)
    config.value = next
    savedTimezone.value = next.timezone
    editors.value = {}
    void editor.value
    loadDraft()
    saved.value = true
  } catch (error) { errorMessage.value = parseApiError(error, error instanceof Error ? error.message : '保存权益失败') }
  finally { loading.value = false }
}
async function removeOverride() { await save(true) }
function buildDraft(): InternalAccessConfig {
  validateFamilies()
  const next = clone(config.value)
  if (Object.keys(families.value).length) next.model_families = clone(families.value)
  else delete next.model_families
  for (const [key, edited] of Object.entries(editors.value)) {
    const colon = key.indexOf(':')
    const type = key.slice(0, colon)
    const id = key.slice(colon + 1)
    if (!id) continue
    if (edited.removed && type === 'user') { delete next.users[id]; continue }
    if (edited.initial === JSON.stringify({ allocation: edited.allocation, models: edited.models, families: edited.families })) continue
    const allocation = serializeAllocation(edited)
    if (type === 'default') next.default = allocation
    else if (type === 'group') next.groups[id] = allocation
    else next.users[id] = allocation
  }
  validateReferences(next)
  return clone(next)
}
function validateFamilies() {
  if (Object.keys(families.value).length > 32) throw new Error('最多可设置 32 个模型家族')
  for (const [id, family] of Object.entries(families.value)) {
    if (!family.name.trim()) throw new Error('家族显示名不能为空')
    if (family.models.length < 1 || family.models.length > 256 || new Set(family.models).size !== family.models.length) throw new Error('每个家族须包含 1..256 个不重复的公开模型')
    // Keep stale catalog members visible/removable, but never introduce unlisted names.
    const retained = new Set(config.value.model_families?.[id]?.models ?? [])
    if (family.models.some(model => !catalogModels.value.includes(model) && !retained.has(model))) throw new Error('家族成员必须从公开模型目录选择')
  }
}
function serializeAllocation(edited: AllocationEditor): Allocation {
  if (edited.allocation.unlimited) return { unlimited: true, models: {} }
  if (edited.models.length + edited.families.length > 15) throw new Error('单个分配的模型与家族子池合计最多 15 个')
  const models: Record<string, ModelAllowance> = Object.create(null)
  const limits: Record<string, ModelAllowance> = Object.create(null)
  for (const row of edited.models) {
    const name = row.model.trim()
    if (!name || Object.prototype.hasOwnProperty.call(models, name)) throw new Error('模型名称不能为空或重复')
    Object.values(row.quota).forEach(validateMoney)
    models[name] = row.quota
  }
  for (const row of edited.families) {
    if (!row.family || !Object.prototype.hasOwnProperty.call(families.value, row.family)) throw new Error('请选择存在的模型家族')
    if (Object.prototype.hasOwnProperty.call(limits, row.family)) throw new Error('家族子池不能重复')
    Object.values(row.quota).forEach(validateMoney)
    limits[row.family] = row.quota
  }
  for (const budget of totalBudgets) validateMoney(edited.allocation[budget.key])
  const allocation = { ...edited.allocation, models }
  if (Object.keys(limits).length) allocation.families = limits
  else delete allocation.families
  return clone(allocation)
}
function validateReferences(policy: InternalAccessConfig) {
  for (const allocation of [policy.default, ...Object.values(policy.groups), ...Object.values(policy.users)]) {
    if (Object.keys(allocation?.families ?? {}).some(id => !Object.prototype.hasOwnProperty.call(families.value, id))) throw new Error('仍有分配引用不存在的家族，请先移除引用')
  }
}
function validateMoney(value: number | undefined) {
  if (value !== undefined && (!Number.isFinite(value) || value < 0 || (value > 0 && value < 0.00000001) || Math.ceil(value * 100_000_000) > Number.MAX_SAFE_INTEGER)) {
    throw new Error('金额额度须为 0 或有效正数，正数最小为 0.00000001 USD')
  }
}
watch([families, editors, () => config.value.timezone], () => { saved.value = false }, { deep: true, flush: 'sync' })
defineExpose({ buildDraft })
onMounted(reload)
</script>
