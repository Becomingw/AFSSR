<template>
  <ol
    class="feedback-timeline"
    aria-label="讨论与状态历史"
  >
    <li
      v-for="entry in entries"
      :key="entry.key"
      class="timeline-entry"
      :class="[{ 'status-entry': entry.kind === 'status' || entry.kind === 'pin' }]"
    >
      <span
        class="timeline-avatar"
        aria-hidden="true"
      >
        <component
          :is="entry.kind === 'pin' ? (entry.pinned ? Pin : PinOff) : entry.to_status === 'resolved' ? CheckCircle2 : entry.to_status === 'closed' ? CircleX : RotateCcw"
          v-if="entry.kind === 'status' || entry.kind === 'pin'"
          :size="16"
        />
        <template v-else>{{ entry.author.slice(0, 1).toUpperCase() }}</template>
      </span>
      <article class="timeline-comment">
        <header>
          <strong>{{ entry.author }}</strong>
          <span
            v-if="entry.kind !== 'status' && entry.kind !== 'pin'"
            class="participant-role"
          >{{ entry.owner ? '发起人' : adminRoleLabel(entry.role) }}</span>
          <span
            v-if="entry.kind === 'pin'"
            class="event-status"
          >{{ entry.pinned ? '置顶了此反馈' : '取消了置顶' }}</span>
          <span
            v-else-if="entry.kind === 'status'"
            class="event-status"
          >{{ feedbackStatusLabel(entry.from_status || 'open') }} → {{ feedbackStatusLabel(entry.to_status || 'open') }}</span>
          <span
            v-else
            class="comment-visibility"
          ><Globe
            v-if="entry.visibility === 'public'"
            :size="12"
          /><Lock
            v-else
            :size="12"
          />{{ entry.visibility === 'public' ? '公开' : '私密' }}</span>
          <time :datetime="new Date(entry.created_at * 1000).toISOString()">{{ feedbackDate(entry.created_at) }}</time>
          <button
            v-if="entry.kind !== 'status' && entry.kind !== 'pin'"
            type="button"
            class="reply-action"
            :aria-label="`回复 ${entry.author}`"
            title="回复"
            @click="emit('reply', entry.author, entry.visibility)"
          >
            <Reply :size="15" />
          </button>
        </header>
        <div
          v-if="entry.content"
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
import { computed } from 'vue'
import { CheckCircle2, CircleX, Globe, Lock, Pin, PinOff, Reply, RotateCcw } from 'lucide-vue-next'
import FeedbackMarkdown from './components/FeedbackMarkdown.vue'
import { adminRoleLabel } from '@/config/adminRoles'
import type { FeedbackEvent, FeedbackSummary, FeedbackVisibility } from '@/api/feedback'
import { feedbackDate, feedbackStatusLabel } from './feedback-view'
const props = defineProps<{ issue: FeedbackSummary & { description: string }, events: FeedbackEvent[], issueBase: string, mentionable?: readonly string[] }>()
const emit = defineEmits<{ reply: [author: string, visibility: FeedbackVisibility] }>()
const entries = computed(() => [
  { key: 'opening', kind: 'created', author: props.issue.owner_username || '已注销用户', role: 'user', owner: true, content: props.issue.description, pinned: false, visibility: props.issue.visibility ?? 'private', created_at: props.issue.created_at, from_status: null, to_status: null },
  ...props.events.filter(event => event.kind !== 'created').map(event => ({ key: `event-${event.seq}`, kind: event.kind, author: event.actor_username || '已注销用户', role: event.actor_role, owner: Boolean(event.actor_id && event.actor_id === props.issue.owner_id), content: event.kind === 'pin' ? null : event.content, pinned: event.kind === 'pin' && event.content === 'pinned', visibility: event.visibility ?? 'private', created_at: event.created_at, from_status: event.from_status, to_status: event.to_status })),
])
</script>

<style scoped>
.feedback-timeline{list-style:none;margin:0;padding:0;position:relative}.feedback-timeline::before{content:'';position:absolute;left:16px;top:20px;bottom:20px;width:1px;background:var(--border)}.timeline-entry{position:relative;display:grid;grid-template-columns:34px minmax(0,1fr);gap:14px;margin-bottom:24px}.timeline-avatar{height:34px;width:34px;border-radius:50%;display:flex;align-items:center;justify-content:center;background:var(--accent);color:var(--primary);font-size:13px;font-weight:600;outline:5px solid var(--background);z-index:1}.timeline-comment{min-width:0;border:1px solid var(--border);border-radius:6px;overflow:hidden}.timeline-comment header{display:flex;align-items:center;flex-wrap:wrap;gap:8px;padding:10px 14px;background:var(--muted);font-size:12px;color:var(--muted-foreground);border-bottom:1px solid var(--border)}.timeline-comment strong{font-weight:600;color:var(--foreground);overflow-wrap:anywhere}.participant-role{border:1px solid var(--border);border-radius:4px;padding:1px 5px;font-size:11px}.comment-visibility,.solution-label{display:inline-flex;align-items:center;gap:4px}.timeline-comment time{margin-left:auto;font-size:11px}.reply-action{display:flex;align-items:center;justify-content:center;width:26px;height:26px;border-radius:4px}.reply-action:hover{background:var(--accent)}.reply-action:focus-visible{outline:2px solid var(--ring)}.comment-body{padding:16px}.feedback-content{margin:0}.status-entry .timeline-comment{border:none;border-radius:0}.status-entry header{padding:5px 0;background:transparent;border:none}.status-entry .comment-body{margin-top:8px;padding:8px 14px;border-left:2px solid color-mix(in srgb, var(--primary) 35%, transparent)}.status-entry .feedback-content{font-size:13px}.solution-label{font-size:11px;color:var(--primary);margin-bottom:5px}.redacted-solution{font-size:12px;color:var(--muted-foreground);padding:5px 0}.event-status{font-size:12px}
.timeline-comment p{font-family:var(--font-sans)}
@media(max-width:640px){.timeline-entry{grid-template-columns:26px minmax(0,1fr);gap:10px}.timeline-avatar{width:26px;height:26px;font-size:11px}.feedback-timeline::before{left:12px}.timeline-comment header{padding:9px 12px;gap:6px}.timeline-comment time{width:100%;margin-left:0;order:2}.comment-body{padding:12px}.reply-action{margin-left:auto}}
</style>
