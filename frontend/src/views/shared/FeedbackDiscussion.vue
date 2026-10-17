<template>
  <ol
    class="feedback-timeline"
    aria-label="讨论与状态历史"
  >
    <li
      v-for="entry in entries"
      :key="entry.key"
      class="timeline-entry"
      :class="[{ 'status-entry': isSystem(entry.kind), 'is-deleted': entry.deleted }]"
    >
      <span
        class="timeline-avatar"
        aria-hidden="true"
      >
        <component
          :is="entry.kind === 'pin' ? (entry.pinned ? Pin : PinOff) : entry.kind === 'issue_edit' ? Pencil : entry.to_status === 'resolved' ? CheckCircle2 : entry.to_status === 'closed' ? CircleX : RotateCcw"
          v-if="isSystem(entry.kind)"
          :size="16"
        />
        <template v-else>{{ entry.author.slice(0, 1).toUpperCase() }}</template>
      </span>
      <article class="timeline-comment">
        <header>
          <strong>{{ entry.author }}</strong>
          <span
            v-if="!isSystem(entry.kind)"
            class="participant-role"
          >{{ entry.owner ? '发起人' : adminRoleLabel(entry.role) }}</span>
          <span
            v-if="entry.kind === 'pin'"
            class="event-status"
          >{{ entry.pinned ? '置顶了此主题' : '取消了置顶' }}</span>
          <span
            v-else-if="entry.kind === 'issue_edit'"
            class="event-status"
          >编辑了主题（{{ editedFields(entry.raw) }}）</span>
          <span
            v-else-if="entry.kind === 'status'"
            class="event-status"
          >{{ feedbackStatusLabel(entry.from_status || 'open') }} → {{ feedbackStatusLabel(entry.to_status || 'open') }}</span>
          <span
            v-else-if="entry.visibility === 'private'"
            class="comment-visibility"
            title="取消私密之前发布的内容，仍只有相关人员能看到"
          ><Lock :size="12" />历史私密</span>
          <time :datetime="new Date(entry.created_at * 1000).toISOString()">{{ feedbackDate(entry.created_at) }}</time>
          <span
            v-if="entry.edited_at"
            class="edited-mark"
            :title="`编辑于 ${feedbackDate(entry.edited_at)}`"
          >已编辑</span>
          <span class="entry-actions">
            <button
              v-if="!isSystem(entry.kind) && !entry.deleted"
              type="button"
              class="reply-action"
              :aria-label="`回复 ${entry.author}`"
              title="回复"
              @click="emit('reply', entry.author)"
            >
              <Reply :size="15" />
            </button>
            <button
              v-if="canEdit(entry)"
              type="button"
              class="reply-action"
              :aria-label="entry.key === 'opening' ? '编辑主题' : `编辑 ${entry.author} 的回复`"
              title="编辑"
              :disabled="busy"
              @click="startEdit(entry)"
            >
              <Pencil :size="14" />
            </button>
            <button
              v-if="canDeleteReply(entry)"
              type="button"
              class="reply-action danger"
              :aria-label="`删除 ${entry.author} 的回复`"
              title="删除"
              :disabled="busy"
              @click="confirming = entry.key"
            >
              <Trash2 :size="14" />
            </button>
          </span>
        </header>
        <div
          v-if="confirming === entry.key"
          class="delete-confirm"
          role="alertdialog"
          aria-label="确认删除回复"
        >
          <span>删除后内容无法恢复，其他人将看到“此回复已删除”。</span>
          <button
            type="button"
            class="confirm-delete"
            :disabled="busy"
            @click="removeReply(entry)"
          >
            确认删除
          </button>
          <button
            type="button"
            :disabled="busy"
            @click="confirming = ''"
          >
            取消
          </button>
        </div>
        <form
          v-if="editing === entry.key"
          class="entry-edit"
          :aria-label="entry.key === 'opening' ? '编辑主题' : '编辑回复'"
          @submit.prevent="saveEdit(entry)"
        >
          <template v-if="entry.key === 'opening'">
            <label>标题<input
              v-model="draftTitle"
              type="text"
              maxlength="200"
              aria-label="主题标题"
              :disabled="busy"
              required
            ></label>
            <label>类型<select
              v-model="draftCategory"
              aria-label="主题类型"
              :disabled="busy"
            ><option
              v-for="option in feedbackCategories"
              :key="option.value"
              :value="option.value"
            >{{ option.label }}</option></select></label>
          </template>
          <FeedbackEditor
            v-model="draftText"
            :aria-label="entry.key === 'opening' ? '主题内容' : '回复内容'"
            :maxlength="20000"
            :disabled="busy"
            :issue-base="issueBase"
            :mentionable="mentionable"
          />
          <div class="entry-edit-actions">
            <button
              type="submit"
              class="primary"
              :disabled="busy || !canSave(entry)"
            >
              保存
            </button>
            <button
              type="button"
              :disabled="busy"
              @click="editing = ''"
            >
              取消
            </button>
          </div>
        </form>
        <p
          v-else-if="entry.deleted"
          class="redacted-solution"
          data-testid="deleted-reply"
        >
          此回复已删除
        </p>
        <div
          v-else-if="entry.content"
          class="comment-body"
        >
          <span
            v-if="entry.kind === 'status' && entry.to_status === 'resolved'"
            class="solution-label"
          >解决说明 <Lock
            v-if="entry.visibility !== 'public'"
            :size="12"
          /></span>
          <FeedbackMarkdown
            :content="entry.content"
            :issue-base="issueBase"
            :mentionable="mentionable"
            class="feedback-content"
            :data-testid="entry.key === 'opening' ? 'feedback-description' : undefined"
          />
        </div>
        <p
          v-else-if="entry.kind === 'status' && entry.to_status === 'resolved'"
          class="redacted-solution"
        >
          解决说明仅对相关参与者可见
        </p>
      </article>
    </li>
  </ol>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { CheckCircle2, CircleX, Lock, Pencil, Pin, PinOff, Reply, RotateCcw, Trash2 } from 'lucide-vue-next'
import FeedbackMarkdown from './components/FeedbackMarkdown.vue'
import FeedbackEditor from './components/FeedbackEditor.vue'
import { adminRoleLabel } from '@/config/adminRoles'
import type { FeedbackCategory, FeedbackEvent, FeedbackSummary } from '@/api/feedback'
import { feedbackCategories, feedbackDate, feedbackStatusLabel } from './feedback-view'

/** What the timeline can ask the page to do; each resolves to whether the change was committed. */
export interface DiscussionActions {
  editIssue: (fields: { title?: string, description?: string, category?: FeedbackCategory }) => Promise<boolean>
  editReply: (seq: number, content: string) => Promise<boolean>
  deleteReply: (seq: number) => Promise<boolean>
}
const props = defineProps<{
  issue: FeedbackSummary & { description: string }
  events: FeedbackEvent[]
  issueBase: string
  mentionable?: readonly string[]
  currentUserId?: string
  /** Administrators moderate every reply and issue. */
  manager?: boolean
  busy?: boolean
  actions?: DiscussionActions
}>()
const emit = defineEmits<{ reply: [author: string] }>()

// Markers only exist for incremental readers; they are not part of the conversation.
const HIDDEN = new Set(['created', 'issue_delete', 'reply_edit', 'reply_delete'])
const FIELD_LABELS: Record<string, string> = { title: '标题', description: '描述', category: '类型' }
type Entry = ReturnType<typeof buildEntries>[number]
function buildEntries() {
  return [
    { key: 'opening', seq: 0, kind: 'created', author: props.issue.owner_username || '已注销用户', actorId: props.issue.owner_id, role: 'user', owner: true, content: props.issue.description as string | null, pinned: false, visibility: props.issue.visibility ?? 'private', created_at: props.issue.created_at, edited_at: props.issue.edited_at ?? null, deleted: false, from_status: null as string | null, to_status: null as string | null, raw: '' },
    ...props.events.filter(event => !HIDDEN.has(event.kind)).map(event => ({ key: `event-${event.seq}`, seq: event.seq, kind: event.kind as string, author: event.actor_username || '已注销用户', actorId: event.actor_id, role: event.actor_role, owner: Boolean(event.actor_id && event.actor_id === props.issue.owner_id), content: event.kind === 'pin' || event.kind === 'issue_edit' ? null : event.content, pinned: event.kind === 'pin' && event.content === 'pinned', visibility: event.visibility ?? 'private', created_at: event.created_at, edited_at: event.edited_at ?? null, deleted: Boolean(event.deleted_at), from_status: event.from_status as string | null, to_status: event.to_status as string | null, raw: event.kind === 'issue_edit' ? event.content ?? '' : '' })),
  ]
}
const entries = computed(buildEntries)
const isSystem = (kind: string) => kind === 'status' || kind === 'pin' || kind === 'issue_edit'
const editedFields = (raw: string) => raw.split(',').map(name => FIELD_LABELS[name] ?? name).filter(Boolean).join('、') || '内容'

const editing = ref(''), confirming = ref('')
const draftText = ref(''), draftTitle = ref(''), draftCategory = ref<FeedbackCategory>('bug')
const isOwner = computed(() => Boolean(props.currentUserId && props.issue.owner_id === props.currentUserId))
// Same rules the server enforces: authors edit their own replies; owners and administrators edit the issue.
function canEdit(entry: Entry) {
  if (!props.actions || entry.deleted) return false
  if (entry.key === 'opening') return isOwner.value || Boolean(props.manager)
  return entry.kind === 'reply' && Boolean(props.currentUserId) && entry.actorId === props.currentUserId
}
function canDeleteReply(entry: Entry) {
  if (!props.actions || entry.deleted || entry.kind !== 'reply') return false
  return Boolean(props.manager) || (Boolean(props.currentUserId) && entry.actorId === props.currentUserId)
}
function startEdit(entry: Entry) {
  confirming.value = ''
  draftText.value = entry.content ?? ''
  if (entry.key === 'opening') { draftTitle.value = props.issue.title; draftCategory.value = props.issue.category }
  editing.value = entry.key
}
function canSave(entry: Entry) {
  const text = draftText.value.trim()
  if (!text) return false
  if (entry.key !== 'opening') return text !== (entry.content ?? '').trim()
  return Boolean(draftTitle.value.trim()) && (text !== props.issue.description.trim() || draftTitle.value.trim() !== props.issue.title || draftCategory.value !== props.issue.category)
}
async function saveEdit(entry: Entry) {
  if (!props.actions || !canSave(entry)) return
  const text = draftText.value.trim()
  const saved = entry.key === 'opening'
    ? await props.actions.editIssue({
      ...(draftTitle.value.trim() !== props.issue.title ? { title: draftTitle.value.trim() } : {}),
      ...(text !== props.issue.description.trim() ? { description: text } : {}),
      ...(draftCategory.value !== props.issue.category ? { category: draftCategory.value } : {}),
    })
    : await props.actions.editReply(entry.seq, text)
  if (saved) editing.value = ''
}
async function removeReply(entry: Entry) {
  if (!props.actions) return
  if (await props.actions.deleteReply(entry.seq)) confirming.value = ''
}
</script>

<style scoped>
.feedback-timeline{list-style:none;margin:0;padding:0;position:relative}.feedback-timeline::before{content:'';position:absolute;left:16px;top:20px;bottom:20px;width:1px;background:var(--border)}.timeline-entry{position:relative;display:grid;grid-template-columns:34px minmax(0,1fr);gap:14px;margin-bottom:24px}.timeline-avatar{height:34px;width:34px;border-radius:50%;display:flex;align-items:center;justify-content:center;background:var(--accent);color:var(--primary);font-size:13px;font-weight:600;outline:5px solid var(--background);z-index:1}.timeline-comment{min-width:0;border:1px solid var(--border);border-radius:6px;overflow:hidden}.timeline-comment header{display:flex;align-items:center;flex-wrap:wrap;gap:8px;padding:10px 14px;background:var(--muted);font-size:12px;color:var(--muted-foreground);border-bottom:1px solid var(--border)}.timeline-comment strong{font-weight:600;color:var(--foreground);overflow-wrap:anywhere}.participant-role{border:1px solid var(--border);border-radius:4px;padding:1px 5px;font-size:11px}.comment-visibility,.solution-label{display:inline-flex;align-items:center;gap:4px}.timeline-comment time{margin-left:auto;font-size:11px}.timeline-comment time + .edited-mark{margin-left:0}.reply-action{display:flex;align-items:center;justify-content:center;width:26px;height:26px;border-radius:4px}.reply-action:hover{background:var(--accent)}.reply-action:focus-visible{outline:2px solid var(--ring)}.comment-body{padding:16px}.feedback-content{margin:0}.status-entry .timeline-comment{border:none;border-radius:0}.status-entry header{padding:5px 0;background:transparent;border:none}.status-entry .comment-body{margin-top:8px;padding:8px 14px;border-left:2px solid color-mix(in srgb, var(--primary) 35%, transparent)}.status-entry .feedback-content{font-size:13px}.solution-label{font-size:11px;color:var(--primary);margin-bottom:5px}.redacted-solution{font-size:12px;color:var(--muted-foreground);padding:5px 0}.event-status{font-size:12px}.entry-actions{display:inline-flex;gap:2px;margin-left:4px}.edited-mark{font-size:11px;border-bottom:1px dotted var(--muted-foreground);cursor:help}.reply-action.danger:hover{background:color-mix(in srgb, var(--destructive, #dc2626) 14%, transparent);color:var(--destructive, #dc2626)}.reply-action:disabled{opacity:.5;cursor:not-allowed}.delete-confirm{display:flex;flex-wrap:wrap;align-items:center;gap:10px;padding:10px 14px;font-size:12px;background:color-mix(in srgb, var(--destructive, #dc2626) 8%, transparent);border-bottom:1px solid var(--border)}.delete-confirm button,.entry-edit-actions button{border:1px solid var(--border);border-radius:4px;padding:3px 10px;font-size:12px}.delete-confirm .confirm-delete{background:var(--destructive, #dc2626);border-color:var(--destructive, #dc2626);color:#fff}.entry-edit{display:grid;gap:10px;padding:14px}.entry-edit label{display:grid;gap:4px;font-size:12px;color:var(--muted-foreground)}.entry-edit input,.entry-edit select{border:1px solid var(--border);border-radius:4px;padding:6px 8px;font-size:14px;color:var(--foreground);background:var(--background)}.entry-edit-actions{display:flex;gap:8px}.entry-edit-actions .primary{background:var(--primary);border-color:var(--primary);color:var(--primary-foreground)}.entry-edit-actions button:disabled,.delete-confirm button:disabled{opacity:.5}.is-deleted .timeline-comment{opacity:.75}.is-deleted .redacted-solution{padding:12px 14px;font-style:italic}
.timeline-comment p{font-family:var(--font-sans)}
@media(max-width:640px){.timeline-entry{grid-template-columns:26px minmax(0,1fr);gap:10px}.timeline-avatar{width:26px;height:26px;font-size:11px}.feedback-timeline::before{left:12px}.timeline-comment header{padding:9px 12px;gap:6px}.timeline-comment time{width:100%;margin-left:0;order:2}.comment-body{padding:12px}.reply-action{margin-left:auto}}
</style>
