<template>
  <!-- eslint-disable vue/no-v-html -->
  <div
    class="feedback-markdown"
    @click="onClick"
    v-html="html"
  />
  <!-- eslint-enable vue/no-v-html -->
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { renderFeedbackMarkdown } from '@/utils/feedbackMarkdown'

const props = defineProps<{
  content: string
  /** 站内反馈页前缀，例如 /admin/feedback */
  issueBase: string
  mentionable?: readonly string[]
}>()
const router = useRouter()
const html = computed(() => renderFeedbackMarkdown(props.content, {
  issueBase: props.issueBase,
  mentionable: props.mentionable,
}))

// #12 style references navigate inside the app instead of reloading the page.
function onClick(event: MouseEvent) {
  const anchor = (event.target as HTMLElement | null)?.closest?.('a[data-issue]')
  if (!anchor || event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) return
  event.preventDefault()
  void router.push(anchor.getAttribute('href') || props.issueBase)
}
</script>

<style>
.feedback-markdown{font-size:14px;line-height:1.85;overflow-wrap:anywhere;min-width:0}
.feedback-markdown>:first-child{margin-top:0}.feedback-markdown>:last-child{margin-bottom:0}
.feedback-markdown p{margin:0 0 .9em}
.feedback-markdown h1,.feedback-markdown h2,.feedback-markdown h3,.feedback-markdown h4{font-weight:600;line-height:1.4;margin:1.2em 0 .6em}
.feedback-markdown h1{font-size:1.35em}.feedback-markdown h2{font-size:1.2em}.feedback-markdown h3,.feedback-markdown h4{font-size:1.05em}
.feedback-markdown ul,.feedback-markdown ol{margin:0 0 .9em;padding-left:1.6em}.feedback-markdown ul{list-style:disc}.feedback-markdown ol{list-style:decimal}
.feedback-markdown li+li{margin-top:.25em}
.feedback-markdown a{color:var(--primary);text-decoration:underline;text-underline-offset:2px}
.feedback-markdown a.fb-ref{text-decoration:none;font-weight:500;white-space:nowrap}.feedback-markdown a.fb-ref:hover{text-decoration:underline}
.feedback-markdown .fb-mention{background:color-mix(in srgb,var(--primary) 13%,transparent);color:var(--primary);border-radius:4px;padding:0 4px;font-weight:500}
.feedback-markdown code{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.9em;background:var(--muted);border-radius:4px;padding:.15em .4em}
.feedback-markdown pre{margin:0 0 1em;padding:12px 14px;background:var(--muted);border:1px solid var(--border);border-radius:6px;overflow:auto}
.feedback-markdown pre code{background:none;padding:0;font-size:12.5px;line-height:1.7}
.feedback-markdown blockquote{margin:0 0 1em;padding:2px 14px;color:var(--muted-foreground);border-left:3px solid var(--border)}
.feedback-markdown hr{border:0;border-top:1px solid var(--border);margin:1.2em 0}
.feedback-markdown img{display:block;max-width:100%;height:auto;margin:.6em 0;border:1px solid var(--border);border-radius:6px;background:var(--muted)}
.feedback-markdown table{border-collapse:collapse;margin:0 0 1em;display:block;overflow:auto;max-width:100%}
.feedback-markdown th,.feedback-markdown td{border:1px solid var(--border);padding:6px 12px;text-align:left}.feedback-markdown th{background:var(--muted);font-weight:600}
.feedback-markdown input[type=checkbox]{margin-right:6px;accent-color:var(--primary)}
</style>
