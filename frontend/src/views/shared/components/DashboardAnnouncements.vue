<script setup lang="ts">
import { computed } from 'vue'
import { RouterLink } from 'vue-router'
import { ArrowRight, Bell, Pin } from 'lucide-vue-next'
import type { Announcement } from '@/api/announcements'
import { getI18nLocale } from '@/i18n'
import { getAnnouncementTypeLabel } from '@/utils/announcement'

const props = defineProps<{
  items: Announcement[]
  loading: boolean
  error: boolean
  allUrl: string
}>()
const emit = defineEmits<{ select: [announcement: Announcement]; refresh: [] }>()

const ordered = computed(() => [...props.items].sort((a, b) =>
  Number(b.is_pinned) - Number(a.is_pinned)
  || b.priority - a.priority
  || Date.parse(b.created_at) - Date.parse(a.created_at),
))
const featured = computed(() => ordered.value[0])
const otherAnnouncements = computed(() => ordered.value.slice(1, 4))
const unreadCount = computed(() => props.items.filter(item => !item.is_read).length)

function excerpt(content: string) {
  return content.replace(/```[\s\S]*?```/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#>*_~`]/g, '')
    .replace(/\s+/g, ' ').trim()
}

function date(value: string) {
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? '' : parsed.toLocaleDateString(getI18nLocale(), { month: 'short', day: 'numeric' })
}
</script>

<template>
  <section
    id="announcements-section"
    class="announcement-spotlight"
    aria-labelledby="dashboard-announcements-title"
    :aria-busy="loading"
  >
    <header class="announcement-spotlight__header">
      <div class="announcement-spotlight__heading">
        <Bell
          class="h-5 w-5"
          aria-hidden="true"
        />
        <h2 id="dashboard-announcements-title">
          系统公告
        </h2>
        <span
          v-if="!loading && !error && unreadCount"
          class="announcement-unread"
        >{{ unreadCount }} 条未读</span>
      </div>
      <RouterLink
        :to="allUrl"
        class="announcement-all"
      >
        查看全部 <ArrowRight
          class="h-4 w-4"
          aria-hidden="true"
        />
      </RouterLink>
    </header>
    <div
      v-if="loading"
      role="status"
      class="announcement-state"
    >
      公告加载中
    </div>
    <div
      v-else-if="error"
      role="status"
      class="announcement-state announcement-state--error"
    >
      <span>公告暂时无法加载</span><button
        type="button"
        @click="emit('refresh')"
      >
        重试
      </button>
    </div>
    <div
      v-else-if="!featured"
      role="status"
      class="announcement-state"
    >
      暂无公告
    </div>
    <div
      v-else
      class="announcement-spotlight__body"
      :class="{ 'announcement-spotlight__body--split': otherAnnouncements.length }"
    >
      <button
        type="button"
        class="announcement-featured"
        :data-tone="featured.is_pinned || featured.requires_ack ? 'important' : featured.type"
        :aria-label="`查看公告：${featured.title}`"
        @click="emit('select', featured)"
      >
        <span class="announcement-featured__meta">
          <span class="announcement-kind">{{ getAnnouncementTypeLabel(featured.type) }}</span>
          <span
            v-if="featured.is_pinned"
            class="announcement-marker"
          ><Pin
            class="h-3.5 w-3.5"
            aria-hidden="true"
          />置顶</span>
          <span v-if="featured.requires_ack">需确认已读</span>
          <time :datetime="featured.created_at">{{ date(featured.created_at) }}</time>
        </span>
        <span
          class="announcement-featured__title"
          translate="no"
        >{{ featured.title }}</span>
        <span
          class="announcement-featured__excerpt"
          translate="no"
        >{{ excerpt(featured.content) }}</span>
        <span class="announcement-featured__footer"><span>{{ featured.is_read ? '已读' : '未读' }}</span><span>阅读全文 <ArrowRight
          class="h-4 w-4"
          aria-hidden="true"
        /></span></span>
      </button>
      <div
        v-if="otherAnnouncements.length"
        class="announcement-recent"
        aria-label="其他公告"
      >
        <button
          v-for="item in otherAnnouncements"
          :key="item.id"
          type="button"
          class="announcement-recent__item"
          :aria-label="`查看公告：${item.title}`"
          @click="emit('select', item)"
        >
          <span class="announcement-recent__meta"><span>{{ item.is_pinned ? '置顶' : getAnnouncementTypeLabel(item.type) }}</span><time :datetime="item.created_at">{{ date(item.created_at) }}</time></span>
          <span
            class="announcement-recent__title"
            translate="no"
          ><span
            v-if="!item.is_read"
            class="announcement-read-dot"
            aria-label="未读"
          />{{ item.title }}</span>
        </button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.announcement-spotlight { min-width: 0; padding-bottom: 24px; border-bottom: 1px solid var(--border); }
.announcement-spotlight__header { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 16px; }
.announcement-spotlight__heading { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.announcement-spotlight__heading > svg { color: var(--primary); }
.announcement-spotlight h2 { font-size: 20px; font-weight: 600; line-height: 28px; }
.announcement-unread { padding: 2px 7px; border-radius: 4px; background: var(--accent); color: var(--accent-foreground); font-size: 12px; line-height: 20px; }
.announcement-all { display: inline-flex; align-items: center; gap: 6px; color: var(--primary); font-size: 13px; font-weight: 500; }
.announcement-all:hover { text-decoration: underline; }
.announcement-spotlight__body { display: grid; gap: 24px; }
.announcement-spotlight__body--split { grid-template-columns: minmax(0, 1.8fr) minmax(240px, 1fr); }
.announcement-featured { display: flex; flex-direction: column; align-items: flex-start; min-width: 0; min-height: 210px; padding: 24px 28px; border-left: 4px solid var(--primary); background: var(--accent); color: var(--foreground); text-align: left; transition: background-color 150ms; }
.announcement-featured[data-tone='important'], .announcement-featured[data-tone='warning'], .announcement-featured[data-tone='maintenance'] { border-left-color: var(--status-warning-fg); background: var(--status-warning-bg); }
.announcement-featured:hover { background: color-mix(in srgb, var(--primary) 6%, var(--card)); }
.announcement-featured__meta, .announcement-recent__meta { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; color: var(--muted-foreground); font-size: 12px; line-height: 20px; }
.announcement-featured__meta time { margin-left: 4px; }
.announcement-kind { color: var(--primary); font-weight: 600; }
.announcement-featured[data-tone='important'] .announcement-kind, .announcement-featured[data-tone='warning'] .announcement-kind, .announcement-featured[data-tone='maintenance'] .announcement-kind { color: var(--status-warning-fg); }
.announcement-marker { display: inline-flex; align-items: center; gap: 4px; }
.announcement-featured__title { display: block; margin-top: 12px; font-size: 24px; font-weight: 600; line-height: 34px; overflow-wrap: anywhere; }
.announcement-featured__excerpt { display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 3; margin-top: 10px; overflow: hidden; color: var(--secondary-foreground); font-size: 14px; line-height: 24px; overflow-wrap: anywhere; }
.announcement-featured__footer { display: flex; justify-content: space-between; flex-wrap: wrap; gap: 12px; width: 100%; margin-top: auto; padding-top: 18px; color: var(--muted-foreground); font-size: 12px; line-height: 20px; }
.announcement-featured__footer > :last-child { display: inline-flex; align-items: center; gap: 6px; color: var(--foreground); font-weight: 500; }
.announcement-recent { display: flex; flex-direction: column; min-width: 0; }
.announcement-recent__item { min-width: 0; padding: 14px 0; border-bottom: 1px solid var(--border); text-align: left; }
.announcement-recent__item:first-child { padding-top: 0; }
.announcement-recent__item:last-child { border-bottom: 0; }
.announcement-recent__item:hover .announcement-recent__title { color: var(--primary); }
.announcement-recent__meta { justify-content: space-between; }
.announcement-recent__title { display: block; margin-top: 6px; font-size: 14px; font-weight: 500; line-height: 22px; overflow-wrap: anywhere; }
.announcement-read-dot { display: inline-block; width: 6px; height: 6px; margin-right: 7px; margin-bottom: 2px; border-radius: 50%; background: var(--primary); }
.announcement-state { display: flex; align-items: center; justify-content: space-between; gap: 16px; min-height: 64px; padding: 16px 20px; background: var(--muted); color: var(--muted-foreground); font-size: 14px; }
.announcement-state--error button { color: var(--primary); text-decoration: underline; }
@media (max-width: 767px) {
  .announcement-spotlight__body--split { grid-template-columns: minmax(0, 1fr); gap: 16px; }
  .announcement-featured { min-height: 210px; padding: 20px; }
  .announcement-featured__title { font-size: 20px; line-height: 30px; }
}
@media (prefers-reduced-motion: reduce) { .announcement-featured { transition: none; } }
</style>
