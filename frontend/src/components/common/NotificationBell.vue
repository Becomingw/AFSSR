<template>
  <div
    ref="root"
    class="notification-bell"
  >
    <button
      type="button"
      class="bell-button"
      :aria-label="unread ? `通知，${unread} 条未读` : '通知'"
      :aria-expanded="open"
      aria-haspopup="true"
      title="通知"
      @click="toggle"
    >
      <Bell :size="16" />
      <span
        v-if="unread"
        class="bell-badge"
        data-testid="notification-badge"
      >{{ unread > 99 ? '99+' : unread }}</span>
    </button>
    <div
      v-if="open"
      class="bell-panel"
      role="dialog"
      aria-label="通知列表"
    >
      <header>
        <strong>通知</strong>
        <button
          type="button"
          class="link"
          :disabled="!unread"
          @click="markAllRead"
        >
          全部标为已读
        </button>
      </header>
      <p
        v-if="loading && !items.length"
        role="status"
        class="bell-empty"
      >
        加载中…
      </p>
      <p
        v-else-if="failed && !items.length"
        role="alert"
        class="bell-empty"
      >
        通知暂时无法加载，请稍后重试
      </p>
      <p
        v-else-if="!items.length"
        class="bell-empty"
      >
        还没有人提到你。在主题或回复里被 @ 时会出现在这里。
      </p>
      <ul v-else>
        <li
          v-for="item in items"
          :key="item.id"
        >
          <button
            type="button"
            class="bell-item"
            :class="{ unread: !item.read_at }"
            @click="openItem(item)"
          >
            <span
              class="dot"
              :aria-label="item.read_at ? '已读' : '未读'"
            />
            <UserAvatar
              :user-id="item.actor_id"
              :version="item.actor_id ? avatars[item.actor_id] : null"
              :name="item.actor_username"
              :size="30"
            />
            <span class="body">
              <span class="line"><strong>{{ item.actor_username || '某位用户' }}</strong> 在 <span class="ref">#{{ item.issue_number }} {{ item.issue_title }}</span> 中提到了你</span>
              <span class="snippet">{{ item.snippet }}</span>
              <time :datetime="new Date(item.created_at * 1000).toISOString()">{{ relative(item.created_at) }}</time>
            </span>
          </button>
        </li>
      </ul>
      <footer v-if="hasMore">
        <button
          type="button"
          class="link"
          :disabled="loading"
          @click="load(true)"
        >
          加载更多
        </button>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Bell } from 'lucide-vue-next'
import { useAuthStore } from '@/stores/auth'
import { canAccessAdminRole } from '@/config/adminRoles'
import { useNotifications } from '@/composables/useNotifications'
import UserAvatar from '@/components/common/UserAvatar.vue'
import type { FeedbackNotification } from '@/api/feedback'

const router = useRouter()
const auth = useAuthStore()
const { unread, items, avatars, hasMore, loading, failed, attach, load, markRead, markAllRead } = useNotifications()
const open = ref(false)
const root = ref<HTMLElement | null>(null)

async function toggle() {
  open.value = !open.value
  if (open.value) await load()
}

async function openItem(item: FeedbackNotification) {
  open.value = false
  if (!item.read_at) void markRead([item.id])
  // Administrators read topics in the console, everybody else on their own page.
  const base = canAccessAdminRole(auth.user?.role) ? '/admin/feedback' : '/dashboard/feedback'
  await router.push(`${base}/${item.issue_number}`)
}

function relative(seconds: number) {
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - seconds)
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`
  return new Date(seconds * 1000).toLocaleDateString()
}

function onPointerDown(event: MouseEvent) {
  if (open.value && root.value && !root.value.contains(event.target as Node)) open.value = false
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && open.value) open.value = false
}
let detach: (() => void) | undefined
onMounted(() => {
  detach = attach()
  document.addEventListener('mousedown', onPointerDown)
  document.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  detach?.()
  document.removeEventListener('mousedown', onPointerDown)
  document.removeEventListener('keydown', onKeydown)
})
</script>

<style scoped>
.notification-bell{position:relative}
.bell-button{position:relative;display:flex;height:36px;width:36px;align-items:center;justify-content:center;border-radius:8px;color:var(--muted-foreground);transition:background .15s,color .15s}
.bell-button:hover{color:var(--foreground);background:color-mix(in srgb,var(--muted) 60%,transparent)}
.bell-button:focus-visible{outline:2px solid var(--ring)}
.bell-badge{position:absolute;top:2px;right:1px;min-width:16px;height:16px;padding:0 4px;border-radius:8px;background:var(--primary);color:var(--primary-foreground);font-size:10px;font-weight:600;line-height:16px;text-align:center}
.bell-panel{position:absolute;right:0;top:calc(100% + 8px);z-index:60;width:min(380px,calc(100vw - 24px));max-height:min(520px,70vh);display:flex;flex-direction:column;border:1px solid var(--border);border-radius:10px;background:var(--background);box-shadow:0 12px 32px rgb(0 0 0 / .14);overflow:hidden}
.bell-panel header,.bell-panel footer{display:flex;align-items:center;justify-content:space-between;padding:10px 14px;font-size:13px}
.bell-panel header{border-bottom:1px solid var(--border)}
.bell-panel footer{border-top:1px solid var(--border);justify-content:center}
.bell-panel ul{list-style:none;margin:0;padding:0;overflow:auto}
.bell-panel li+li{border-top:1px solid var(--border)}
.link{font-size:12px;color:var(--primary)}
.link:disabled{color:var(--muted-foreground);cursor:default}
.bell-empty{padding:28px 18px;text-align:center;font-size:13px;color:var(--muted-foreground);line-height:1.7}
.bell-item{display:grid;grid-template-columns:10px 30px minmax(0,1fr);align-items:start;gap:10px;width:100%;padding:12px 14px;text-align:left}
.bell-item:hover{background:var(--muted)}
.bell-item:focus-visible{outline:2px solid var(--ring);outline-offset:-2px}
.dot{width:8px;height:8px;margin-top:6px;border-radius:50%;background:transparent}
.bell-item.unread .dot{background:var(--primary)}
.body{display:grid;gap:3px;min-width:0}
.line{font-size:13px;line-height:1.5;overflow-wrap:anywhere}
.ref{color:var(--primary)}
.snippet{font-size:12px;color:var(--muted-foreground);display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;overflow-wrap:anywhere}
.body time{font-size:11px;color:var(--muted-foreground)}
</style>
