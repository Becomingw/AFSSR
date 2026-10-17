<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="focus-layer"
      role="dialog"
      aria-modal="true"
      aria-label="专注模式"
    >
      <header class="focus-bar">
        <span class="focus-label"><Maximize2 :size="14" />专注模式</span>
        <span class="focus-count">{{ description.length.toLocaleString() }} 字符</span>
        <button
          type="button"
          class="focus-exit"
          title="退出专注模式（Esc）"
          @click="emit('close')"
        >
          <Minimize2 :size="14" />退出 <kbd>Esc</kbd>
        </button>
      </header>
      <main class="focus-body">
        <input
          :value="title"
          class="focus-title"
          type="text"
          maxlength="200"
          placeholder="主题标题"
          aria-label="主题标题"
          :disabled="busy"
          @input="emit('update:title', ($event.target as HTMLInputElement).value)"
        >
        <FeedbackEditor
          ref="editor"
          class="focus-editor"
          :model-value="description"
          aria-label="主题内容"
          placeholder="现象、复现步骤与预期结果；请勿包含密钥、密码或私人对话。支持 Markdown，用 @用户名 可以提醒对方。"
          :maxlength="20000"
          :disabled="busy"
          :issue-base="issueBase"
          :mentionable="mentionable"
          :exclude-user="excludeUser"
          @update:model-value="emit('update:description', $event)"
        />
      </main>
      <footer class="focus-footer">
        <label class="focus-category">类型<select
          :value="category"
          aria-label="主题类型"
          :disabled="busy"
          @change="emit('update:category', ($event.target as HTMLSelectElement).value as FeedbackCategory)"
        ><option
          v-for="option in categories"
          :key="option.value"
          :value="option.value"
        >{{ option.label }}</option></select></label>
        <p
          v-if="error"
          role="alert"
          class="focus-error"
        >
          {{ error }}
        </p>
        <div class="focus-actions">
          <button
            type="button"
            :disabled="busy"
            @click="emit('close')"
          >
            返回表单
          </button>
          <button
            type="button"
            class="focus-submit"
            :disabled="busy || !title.trim() || !description.trim()"
            @click="emit('submit')"
          >
            发起主题
          </button>
        </div>
      </footer>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { Maximize2, Minimize2 } from 'lucide-vue-next'
import FeedbackEditor from './FeedbackEditor.vue'
import type { FeedbackCategory } from '@/api/feedback'

const props = defineProps<{
  open: boolean
  title: string
  description: string
  category: FeedbackCategory
  busy?: boolean
  categories: ReadonlyArray<{ value: string, label: string }>
  issueBase: string
  mentionable?: readonly string[]
  excludeUser?: string
  error?: string
}>()
const emit = defineEmits<{
  close: []
  submit: []
  'update:title': [value: string]
  'update:description': [value: string]
  'update:category': [value: FeedbackCategory]
}>()
const editor = ref<{ focus: () => void } | null>(null)

// Esc leaves focus mode only: it is swallowed before the surrounding dialog can treat it as "close".
function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || props.busy) return
  event.preventDefault()
  event.stopImmediatePropagation()
  emit('close')
}
let locked = false
function lock() {
  if (locked) return
  locked = true
  document.body.style.overflow = 'hidden'
  window.addEventListener('keydown', onKeydown, true)
}
function unlock() {
  if (!locked) return
  locked = false
  document.body.style.overflow = ''
  window.removeEventListener('keydown', onKeydown, true)
}
watch(() => props.open, async (open) => {
  if (!open) return unlock()
  lock()
  await nextTick()
  editor.value?.focus()
}, { immediate: true })
onBeforeUnmount(unlock)
</script>

<style scoped>
.focus-layer{position:fixed;inset:0;z-index:2000;display:flex;flex-direction:column;background:var(--background);color:var(--foreground)}
.focus-bar,.focus-footer{display:flex;align-items:center;gap:16px;padding:10px clamp(16px,4vw,48px);border-color:var(--border)}
.focus-bar{border-bottom:1px solid var(--border);font-size:12px;color:var(--muted-foreground)}
.focus-label{display:inline-flex;align-items:center;gap:6px;font-weight:600;color:var(--foreground)}
.focus-count{margin-left:auto}
.focus-exit{display:inline-flex;align-items:center;gap:6px;border:1px solid var(--border);border-radius:5px;padding:4px 10px;font-size:12px}
.focus-exit:hover{background:var(--muted)}
.focus-exit kbd{font-family:inherit;font-size:10px;border:1px solid var(--border);border-radius:3px;padding:0 4px;color:var(--muted-foreground)}
.focus-body{flex:1;min-height:0;overflow:auto;width:100%;max-width:860px;margin:0 auto;padding:clamp(20px,5vh,56px) clamp(16px,4vw,32px);display:flex;flex-direction:column;gap:20px}
.focus-title{width:100%;border:0;border-bottom:1px solid var(--border);background:transparent;padding:6px 0 14px;font-size:clamp(22px,3vw,30px);font-weight:600;line-height:1.3;outline:none;color:inherit}
.focus-title:focus{border-bottom-color:var(--primary)}
.focus-title::placeholder{color:var(--muted-foreground);font-weight:400}
.focus-editor{flex:1;min-height:0}
.focus-editor :deep(textarea){min-height:52vh;font-size:16px;line-height:1.85;padding:16px 18px}
.focus-footer{border-top:1px solid var(--border);flex-wrap:wrap}
.focus-category{display:inline-flex;align-items:center;gap:8px;font-size:12px;color:var(--muted-foreground)}
.focus-category select{height:34px;border:1px solid var(--border);border-radius:5px;background:var(--background);padding:0 10px;font-size:13px;color:var(--foreground)}
.focus-error{font-size:12px;color:var(--destructive);flex:1;min-width:0;overflow-wrap:anywhere}
.focus-actions{display:flex;gap:10px;margin-left:auto}
.focus-actions button{border:1px solid var(--border);border-radius:5px;padding:7px 16px;font-size:13px}
.focus-actions button:disabled{opacity:.5;cursor:not-allowed}
.focus-actions .focus-submit{background:var(--primary);border-color:var(--primary);color:var(--primary-foreground);font-weight:600}
@media(max-width:640px){.focus-bar,.focus-footer{padding:8px 14px;gap:10px}.focus-body{padding:16px 14px}.focus-editor :deep(textarea){min-height:44vh;font-size:15px}}
</style>
