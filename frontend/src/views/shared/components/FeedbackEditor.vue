<template>
  <div class="feedback-editor">
    <div
      class="editor-tabs"
      role="tablist"
    >
      <button
        type="button"
        role="tab"
        :aria-selected="!preview"
        @click="preview = false"
      >
        编辑
      </button>
      <button
        type="button"
        role="tab"
        :aria-selected="preview"
        @click="preview = true"
      >
        预览
      </button>
      <span class="editor-hint">支持 Markdown 与图片链接 · <b>@</b>管理员 · <b>#</b>编号引用其他反馈</span>
    </div>
    <textarea
      v-show="!preview"
      :id="id"
      ref="textarea"
      :value="modelValue"
      :aria-label="ariaLabel"
      :placeholder="placeholder"
      :maxlength="maxlength"
      :disabled="disabled"
      :required="required"
      @input="onInput"
      @keydown="onKeydown"
      @click="refreshSuggestions"
      @keyup="onKeyup"
      @blur="closeSoon"
    />
    <ul
      v-if="open && !preview"
      class="mention-list"
      role="listbox"
      aria-label="可提及的管理员"
    >
      <li
        v-for="(name, index) in suggestions"
        :key="name"
        role="option"
        :aria-selected="index === active"
      >
        <button
          type="button"
          :class="{ active: index === active }"
          @mousedown.prevent="pick(name)"
        >
          @{{ name }}
        </button>
      </li>
    </ul>
    <div
      v-if="preview"
      class="editor-preview"
      aria-label="预览"
    >
      <FeedbackMarkdown
        v-if="modelValue.trim()"
        :content="modelValue"
        :issue-base="issueBase"
        :mentionable="mentionable"
      />
      <p
        v-else
        class="preview-empty"
      >
        没有可预览的内容
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import FeedbackMarkdown from './FeedbackMarkdown.vue'

const props = withDefaults(defineProps<{
  modelValue: string
  id?: string
  ariaLabel?: string
  placeholder?: string
  maxlength?: number
  disabled?: boolean
  required?: boolean
  issueBase: string
  mentionable?: readonly string[]
}>(), { id: undefined, ariaLabel: undefined, placeholder: '', maxlength: 20000, mentionable: () => [] })
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const textarea = ref<HTMLTextAreaElement | null>(null)
const preview = ref(false)
const query = ref<string | null>(null)
const active = ref(0)

const MENTION_BEFORE_CARET = /(?:^|[^\p{L}\p{N}_])@([\p{L}\p{N}_.-]{0,32})$/u

const suggestions = computed(() => {
  if (query.value === null) return []
  const needle = query.value.toLowerCase()
  return props.mentionable
    .filter(name => name.toLowerCase().includes(needle))
    .sort((a, b) => Number(b.toLowerCase().startsWith(needle)) - Number(a.toLowerCase().startsWith(needle)))
    .slice(0, 6)
})
const open = computed(() => suggestions.value.length > 0)

function refreshSuggestions() {
  const el = textarea.value
  if (!el || props.disabled) { query.value = null; return }
  const before = el.value.slice(0, el.selectionStart ?? 0)
  const match = MENTION_BEFORE_CARET.exec(before)
  query.value = match ? match[1] : null
  active.value = 0
}
function onInput(event: Event) {
  emit('update:modelValue', (event.target as HTMLTextAreaElement).value)
  refreshSuggestions()
}
function onKeyup(event: KeyboardEvent) {
  if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) refreshSuggestions()
}
function onKeydown(event: KeyboardEvent) {
  if (!open.value) return
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    const step = event.key === 'ArrowDown' ? 1 : -1
    active.value = (active.value + step + suggestions.value.length) % suggestions.value.length
  } else if (event.key === 'Enter' || event.key === 'Tab') {
    event.preventDefault()
    pick(suggestions.value[active.value])
  } else if (event.key === 'Escape') {
    event.preventDefault()
    query.value = null
  }
}
function closeSoon() { query.value = null }

async function pick(name: string) {
  const el = textarea.value
  if (!el) return
  const caret = el.selectionStart ?? el.value.length
  const before = el.value.slice(0, caret)
  const match = MENTION_BEFORE_CARET.exec(before)
  if (!match) return
  const start = caret - match[1].length - 1
  const inserted = `@${name} `
  const next = `${el.value.slice(0, start)}${inserted}${el.value.slice(caret)}`
  emit('update:modelValue', next)
  query.value = null
  await nextTick()
  el.focus()
  el.setSelectionRange(start + inserted.length, start + inserted.length)
}

defineExpose({ focus: () => { preview.value = false; void nextTick(() => textarea.value?.focus()) } })
</script>

<style scoped>
textarea{display:block;width:100%;min-height:140px;border:1px solid var(--border);border-radius:6px;background:var(--background);padding:12px 14px;font-size:14px;line-height:24px;resize:vertical}textarea:focus-visible{outline:2px solid var(--ring);outline-offset:2px}
.feedback-editor{position:relative;display:block;min-width:0}
.editor-tabs{display:flex;align-items:center;gap:2px;margin-bottom:8px;border-bottom:1px solid var(--border)}
.editor-tabs button{font-size:12px;padding:6px 12px;color:var(--muted-foreground);border-bottom:2px solid transparent;margin-bottom:-1px}
.editor-tabs button[aria-selected=true]{color:var(--foreground);border-bottom-color:var(--primary);font-weight:600}
.editor-hint{margin-left:auto;font-size:11px;color:var(--muted-foreground);padding-right:4px}.editor-hint b{color:var(--primary)}
.mention-list{position:absolute;z-index:20;left:12px;bottom:12px;list-style:none;margin:0;padding:4px;min-width:160px;max-width:calc(100% - 24px);background:var(--popover,var(--background));border:1px solid var(--border);border-radius:8px;box-shadow:0 8px 24px rgba(0,0,0,.12)}
.mention-list button{display:block;width:100%;text-align:left;font-size:13px;padding:6px 10px;border-radius:5px}.mention-list button.active,.mention-list button:hover{background:var(--accent)}
.editor-preview{min-height:140px;border:1px solid var(--border);border-radius:6px;padding:12px 14px;background:var(--background)}
.preview-empty{font-size:13px;color:var(--muted-foreground)}
@media(max-width:640px){.editor-hint{display:none}}
</style>
