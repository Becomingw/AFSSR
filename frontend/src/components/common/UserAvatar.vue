<template>
  <span
    class="user-avatar"
    :class="{ 'has-picture': showPicture }"
    :style="{ width: `${size}px`, height: `${size}px`, fontSize: `${Math.round(size * 0.4)}px` }"
    :title="name || undefined"
  >
    <img
      v-if="showPicture"
      :src="src ?? undefined"
      :alt="name ? `${name} 的头像` : ''"
      loading="lazy"
      decoding="async"
      referrerpolicy="no-referrer"
      @error="failed = true"
    >
    <template v-else>{{ initials }}</template>
  </span>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { avatarSrc } from '@/api/avatar'

const props = withDefaults(defineProps<{
  userId?: string | null
  /** Picture version from the server; no version means no picture. */
  version?: number | null
  name?: string | null
  size?: number
}>(), { userId: null, version: null, name: '', size: 34 })

const failed = ref(false)
const src = computed(() => avatarSrc(props.userId, props.version))
const showPicture = computed(() => Boolean(src.value) && !failed.value)
const initials = computed(() => (props.name?.trim() ? Array.from(props.name.trim())[0]!.toUpperCase() : '?'))
// A new version is a new picture: give it a fresh chance to load.
watch(src, () => { failed.value = false })
</script>

<style scoped>
.user-avatar{display:inline-flex;flex:none;align-items:center;justify-content:center;border-radius:50%;overflow:hidden;background:var(--accent);color:var(--primary);font-weight:600;line-height:1;user-select:none}
.user-avatar img{width:100%;height:100%;object-fit:cover;display:block}
</style>
