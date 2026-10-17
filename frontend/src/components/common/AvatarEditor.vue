<template>
  <div class="avatar-editor">
    <UserAvatar
      :user-id="auth.user?.id"
      :version="auth.user?.avatar_version"
      :name="auth.user?.username"
      :size="72"
    />
    <div class="avatar-controls">
      <div class="avatar-buttons">
        <button
          type="button"
          class="primary"
          :disabled="busy"
          @click="chooser?.click()"
        >
          {{ busy ? '处理中…' : hasPicture ? '更换头像' : '上传头像' }}
        </button>
        <button
          v-if="hasPicture"
          type="button"
          :disabled="busy"
          @click="remove"
        >
          移除头像
        </button>
      </div>
      <p class="avatar-hint">
        支持 PNG、JPEG、WebP、GIF。图片会在你的浏览器里裁成正方形并压缩，只保存缩小后的版本；头像对所有已登录用户可见。
      </p>
      <p
        v-if="problem"
        role="alert"
        class="avatar-problem"
      >
        {{ problem }}
      </p>
    </div>
    <input
      ref="chooser"
      type="file"
      hidden
      :accept="AVATAR_ACCEPT"
      aria-label="选择头像图片"
      @change="onChosen"
    >
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import UserAvatar from '@/components/common/UserAvatar.vue'
import { removeAvatar, uploadAvatar } from '@/api/avatar'
import { AVATAR_ACCEPT, AvatarImageError, prepareAvatar } from '@/utils/avatarImage'
import { useAuthStore } from '@/stores/auth'
import { useToast } from '@/composables/useToast'

const auth = useAuthStore()
const { success } = useToast()
const chooser = ref<HTMLInputElement | null>(null)
const busy = ref(false)
const problem = ref('')
const hasPicture = computed(() => Boolean(auth.user?.avatar_version))

function setVersion(version: number | null) {
  if (auth.user) auth.user = { ...auth.user, avatar_version: version }
}

function describe(reason: unknown): string {
  if (reason instanceof AvatarImageError) return reason.message
  const status = (reason as { response?: { status?: number } })?.response?.status
  if (status === 413) return '图片太大了，请换一张更小的图片'
  if (status === 415) return '只支持 PNG、JPEG、WebP 或 GIF 图片'
  return '头像保存失败，请稍后重试'
}

async function onChosen(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = '' // choosing the same file again must work
  if (!file || busy.value) return
  busy.value = true
  problem.value = ''
  try {
    const uploaded = await uploadAvatar(await prepareAvatar(file))
    setVersion(uploaded.version)
    success('头像已更新')
  } catch (reason) {
    problem.value = describe(reason)
  } finally {
    busy.value = false
  }
}

async function remove() {
  if (busy.value) return
  busy.value = true
  problem.value = ''
  try {
    await removeAvatar()
    setVersion(null)
    success('头像已移除')
  } catch (reason) {
    problem.value = describe(reason)
  } finally {
    busy.value = false
  }
}
</script>

<style scoped>
.avatar-editor{display:flex;align-items:center;gap:16px;flex-wrap:wrap}
.avatar-controls{display:grid;gap:8px;min-width:0;flex:1}
.avatar-buttons{display:flex;gap:8px;flex-wrap:wrap}
.avatar-buttons button{border:1px solid var(--border);border-radius:6px;padding:6px 14px;font-size:13px}
.avatar-buttons button:hover:not(:disabled){background:var(--muted)}
.avatar-buttons button:disabled{opacity:.55;cursor:not-allowed}
.avatar-buttons button:focus-visible{outline:2px solid var(--ring)}
.avatar-buttons .primary{background:var(--primary);border-color:var(--primary);color:var(--primary-foreground)}
.avatar-buttons .primary:hover:not(:disabled){background:var(--primary);opacity:.9}
.avatar-hint{font-size:12px;line-height:1.6;color:var(--muted-foreground)}
.avatar-problem{font-size:12px;color:var(--destructive)}
</style>
