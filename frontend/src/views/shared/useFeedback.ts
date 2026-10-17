import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { canAccessAdminRole } from '@/config/adminRoles'
import { listFeedback, getFeedback, createFeedback, commandFeedback, listMentionableAdmins, type FeedbackAction, type FeedbackCategory, type FeedbackDetail, type FeedbackSummary, type FeedbackVisibility } from '@/api/feedback'

export function useFeedback() {
  const route = useRoute(), router = useRouter(), auth = useAuthStore()
  const admin = computed(() => route.path.startsWith('/admin/'))
  const manager = computed(() => canAccessAdminRole(auth.user?.role))
  const canCreate = computed(() => manager.value || (!admin.value && auth.user?.role === 'user'))
  const base = computed(() => admin.value ? '/admin/feedback' : '/dashboard/feedback')
  const selectedId = computed(() => String(route.params.issueId || ''))
  const status = ref(''), category = ref(''), search = ref(''), page = ref(0), total = ref(0)
  const items = ref<FeedbackSummary[]>([]), detail = ref<FeedbackDetail>()
  const loading = ref(false), detailLoading = ref(false), busy = ref(false), creating = ref(false), closing = ref(false)
  const error = ref(''), notice = ref(''), draftError = ref(''), replyText = ref(''), replyingTo = ref('')
  const replyVisibility = ref<FeedbackVisibility>('private')
  const replyInput = ref<{ focus: () => void } | null>(null)
  const mentionable = ref<string[]>([])
  const draft = reactive({ title: '', description: '', category: 'bug' as FeedbackCategory, model: '', request_id: '', visibility: 'private' as FeedbackVisibility })
  const diagnosticId = ref(''), includeDiagnostics = ref(false), attachReply = ref(false)
  const canManageIssue = computed(() => Boolean(detail.value && (manager.value || (auth.user?.id && detail.value.issue.owner_id === auth.user.id))))
  const publicIssue = computed(() => detail.value?.issue.visibility === 'public')
  function createFromDiagnostic() { if (canCreate.value) { draft.request_id = diagnosticId.value.trim(); includeDiagnostics.value = true; creating.value = true } }
  watch(() => route.query?.request_id, (id) => {
    if (typeof id !== 'string' || !/^[a-zA-Z0-9_.-]{1,128}$/.test(id)) return
    diagnosticId.value = id
    if (canCreate.value) createFromDiagnostic()
  }, { immediate: true })
  watch(diagnosticId, () => { attachReply.value = false })
  watch(creating, (open) => { if (!open) includeDiagnostics.value = false })
  let listGeneration = 0, detailGeneration = 0, displayedId = ''
  let pending: { signature: string, id: string } | undefined
  function commandId(value: unknown) {
    const signature = JSON.stringify(value)
    if (pending?.signature !== signature) pending = { signature, id: crypto.randomUUID() }
    return pending.id
  }
  function problem(reason: unknown) {
    const code = (reason as { response?: { status?: number } })?.response?.status
    if (code === 409) return '反馈已被更新，请刷新详情后重试；未覆盖其他人的修改'
    if (code === 404) return '反馈不存在或你没有查看权限'
    if (code === 403) return '你没有执行此操作的权限'
    return '操作失败，请检查网络或访问权限后重试'
  }
  async function loadList() {
    const generation = ++listGeneration; loading.value = true; error.value = ''
    try {
      const result = await listFeedback(admin.value, { status: status.value || undefined, category: category.value || undefined, search: search.value.trim() || undefined, skip: page.value * 20, limit: 20 })
      if (generation === listGeneration) { items.value = result.items; total.value = result.total }
    } catch (reason) { if (generation === listGeneration) { items.value = []; error.value = problem(reason) } }
    finally { if (generation === listGeneration) loading.value = false }
  }
  async function loadDetail() {
    const generation = ++detailGeneration, id = selectedId.value
    detail.value = undefined; closing.value = false
    if (displayedId !== id) {
      replyText.value = ''; replyVisibility.value = 'private'; replyingTo.value = ''; attachReply.value = false
      displayedId = id
    }
    if (!id) { detailLoading.value = false; return }
    detailLoading.value = true; error.value = ''
    try {
      const result = await getFeedback(admin.value, id)
      if (generation === detailGeneration) detail.value = result
    } catch (reason) { if (generation === detailGeneration) error.value = problem(reason) }
    finally { if (generation === detailGeneration) detailLoading.value = false }
  }
  async function refresh() { await Promise.all([loadList(), loadDetail()]) }
  function filterChanged() { page.value = 0; void loadList() }
  async function openIssue(id: string) { if (!busy.value) await router.push(`${base.value}/${encodeURIComponent(id)}`) }
  async function showList() { if (!busy.value) await router.push(base.value) }
  async function replyTo(author: string, visibility: FeedbackVisibility) {
    replyingTo.value = author
    if (visibility !== 'public') replyVisibility.value = 'private'
    if (!replyText.value) replyText.value = `@${author} `
    await nextTick(); replyInput.value?.focus()
  }
  async function moreEvents() {
    if (!detail.value || busy.value) return
    const id = selectedId.value, cursor = detail.value.next_event_cursor || 0; busy.value = true
    try {
      const result = await getFeedback(admin.value, id, cursor)
      if (selectedId.value === id && detail.value) {
        const events = new Map([...detail.value.events, ...result.events].map(event => [event.seq, event]))
        detail.value = { ...result, events: [...events.values()].sort((a, b) => a.seq - b.seq) }
      }
    } catch (reason) { error.value = problem(reason) }
    finally { busy.value = false }
  }
  async function submit() {
    if (!canCreate.value || busy.value) return
    const payload = { title: draft.title.trim(), description: draft.description.trim(), category: draft.category, model: draft.model.trim() || undefined, request_id: draft.request_id.trim() || undefined, visibility: draft.visibility, include_diagnostics: includeDiagnostics.value && Boolean(draft.request_id.trim()) }
    if (!payload.title || !payload.description) { draftError.value = '请填写标题和问题描述'; return }
    if (new TextEncoder().encode(payload.description).length > 20000) { draftError.value = '描述最多 20,000 字节'; return }
    busy.value = true; draftError.value = ''; notice.value = ''
    try {
      const result = await createFeedback({ ...payload, client_request_id: commandId(payload) }, admin.value)
      creating.value = false; Object.assign(draft, { title: '', description: '', category: 'bug', model: '', request_id: '', visibility: 'private' })
      pending = undefined; page.value = 0; status.value = ''; category.value = ''; search.value = ''; notice.value = '反馈已提交'
      await router.push(`${base.value}/${result.issue.id}`); await loadList()
    } catch (reason) { draftError.value = problem(reason) }
    finally { busy.value = false }
  }
  async function mutate(action: FeedbackAction) {
    if (!detail.value || busy.value || detailLoading.value) return
    const content = action.action === 'reply' ? action.content : action.action === 'set_status' ? action.comment || '' : ''
    if (new TextEncoder().encode(content).length > 20000) { error.value = '评论最多 20,000 字节'; return }
    const id = selectedId.value
    const value = { ...action, expected_version: detail.value.issue.version, visibility: publicIssue.value ? replyVisibility.value : 'private' as FeedbackVisibility, ...(action.action === 'reply' && attachReply.value && diagnosticId.value.trim() ? { diagnostic_request_id: diagnosticId.value.trim() } : {}) }
    busy.value = true; error.value = ''; notice.value = ''
    let committed = false
    try {
      const result = await commandFeedback(admin.value, id, { ...value, client_request_id: commandId({ id, ...value }) })
      pending = undefined; committed = true
      notice.value = action.action === 'reply' ? '回复已发送' : action.action === 'set_pinned' ? (action.pinned ? '已置顶' : '已取消置顶') : '状态已更新'
      if (selectedId.value === id && detail.value) {
        detail.value.issue = result.issue; closing.value = false
        if (action.action === 'reply' || (action.action === 'set_status' && action.comment?.trim())) {
          replyText.value = ''; replyingTo.value = ''; replyVisibility.value = 'private'; attachReply.value = false
        }
        const after = detail.value.has_more ? result.issue.activity_seq - 1 : Math.max(detail.value.next_event_cursor || 0, ...detail.value.events.map(event => event.seq), 0)
        const latest = await getFeedback(admin.value, id, after)
        if (selectedId.value === id && detail.value) {
          const events = new Map([...detail.value.events, ...latest.events].map(event => [event.seq, event]))
          detail.value.issue = latest.issue
          detail.value.events = [...events.values()].sort((a, b) => a.seq - b.seq)
          if (!detail.value.has_more) { detail.value.has_more = latest.has_more; detail.value.next_event_cursor = latest.next_event_cursor }
        }
      }
      await loadList()
    } catch (reason) { error.value = committed ? '内容已保存，但刷新失败，请刷新核对，不要重复提交' : problem(reason) }
    finally { busy.value = false }
  }
  async function reply() { if (replyText.value.trim()) await mutate({ action: 'reply', content: replyText.value.trim() }) }
  async function resolve() {
    if (!canManageIssue.value) return
    if (!replyText.value.trim()) { error.value = '请填写最终解决说明'; return }
    if (attachReply.value) { error.value = '请先发送附带诊断的回复，再填写最终解决说明'; return }
    await mutate({ action: 'set_status', status: 'resolved', comment: replyText.value.trim() })
  }
  async function togglePin() { if (manager.value && detail.value) await mutate({ action: 'set_pinned', pinned: !detail.value.issue.pinned_at }) }
  async function loadMentionable() { try { mentionable.value = await listMentionableAdmins(admin.value) } catch { mentionable.value = [] } }
  async function reopen() { if (canManageIssue.value) await mutate({ action: 'set_status', status: 'open' }) }
  async function closeIssue() { if (manager.value) await mutate({ action: 'set_status', status: 'closed' }) }
  watch(() => [selectedId.value, admin.value], () => { void loadDetail() })
  watch(admin, () => { items.value = []; total.value = 0; page.value = 0; void loadList(); void loadMentionable() })
  onMounted(() => { void refresh(); void loadMentionable() })
  return { admin, manager, canCreate, selectedId, status, category, search, page, total, items, detail, loading, detailLoading, busy, creating, closing, error, notice, draftError, replyText, replyingTo, replyVisibility, replyInput, mentionable, base, togglePin, draft, diagnosticId, includeDiagnostics, attachReply, canManageIssue, publicIssue, createFromDiagnostic, loadList, refresh, filterChanged, openIssue, showList, replyTo, moreEvents, submit, reply, resolve, reopen, closeIssue }
}
