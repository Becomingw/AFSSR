import { computed, ref } from 'vue'
import { listNotifications, markNotificationsRead, type FeedbackNotification } from '@/api/feedback'

const POLL_MS = 60_000

// One shared state for every bell in the layout (desktop and mobile headers).
const unread = ref(0)
const items = ref<FeedbackNotification[]>([])
const nextBefore = ref<number | null>(null)
const loading = ref(false)
const failed = ref(false)
let timer: ReturnType<typeof setInterval> | undefined
let users = 0
let generation = 0

async function refreshCount() {
  try {
    unread.value = (await listNotifications({ unread: true, limit: 1 })).unread_count
  } catch {
    // A failed poll keeps the last known count; the next tick tries again.
  }
}

async function load(more = false) {
  const current = ++generation
  loading.value = true
  failed.value = false
  try {
    const page = await listNotifications({ limit: 20, before: more ? nextBefore.value ?? undefined : undefined })
    if (current !== generation) return
    items.value = more ? [...items.value, ...page.items] : page.items
    nextBefore.value = page.next_before
    unread.value = page.unread_count
  } catch {
    if (current === generation) failed.value = true
  } finally {
    if (current === generation) loading.value = false
  }
}

async function markRead(ids: number[]) {
  const target = new Set(ids)
  const now = Math.floor(Date.now() / 1000)
  items.value = items.value.map(item => target.has(item.id) && !item.read_at ? { ...item, read_at: now } : item)
  try {
    unread.value = await markNotificationsRead({ ids })
  } catch {
    await refreshCount()
  }
}

async function markAllRead() {
  const now = Math.floor(Date.now() / 1000)
  items.value = items.value.map(item => item.read_at ? item : { ...item, read_at: now })
  try {
    unread.value = await markNotificationsRead({ all: true })
  } catch {
    await refreshCount()
  }
}

/** Start polling while at least one bell is mounted; returns the stop function. */
function attach() {
  users += 1
  if (users === 1) {
    void refreshCount()
    timer = setInterval(() => { if (!document.hidden) void refreshCount() }, POLL_MS)
  }
  return () => {
    users = Math.max(0, users - 1)
    if (users === 0 && timer) { clearInterval(timer); timer = undefined }
  }
}

export function useNotifications() {
  return {
    unread: computed(() => unread.value),
    items: computed(() => items.value),
    hasMore: computed(() => nextBefore.value !== null),
    loading: computed(() => loading.value),
    failed: computed(() => failed.value),
    attach,
    refreshCount,
    load,
    markRead,
    markAllRead,
  }
}

/** Test hook: forget everything between tests. */
export function resetNotificationsForTests() {
  unread.value = 0
  items.value = []
  nextBefore.value = null
  loading.value = false
  failed.value = false
  generation = 0
  users = 0
  if (timer) { clearInterval(timer); timer = undefined }
}
