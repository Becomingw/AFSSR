<template>
  <div data-ui="card" :class="cardClass">
    <slot />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { cn } from '@/lib/utils'

interface Props {
  variant?: 'default' | 'glass' | 'elevated' | 'interactive' | 'subtle' | 'inset'
  class?: string
}

const props = withDefaults(defineProps<Props>(), {
  variant: 'default',
  class: undefined,
})

// 标准卡片变体定义
const variants = {
  // 默认卡片 - 纯色背景,标准边框,用于主要内容容器
  default: 'rounded-2xl border border-border bg-card text-card-foreground shadow-sm',

  // 玻璃态卡片 - 半透明背景+模糊效果,用于嵌套内容/次要层级
  glass: 'rounded-2xl border border-border bg-card/50 text-card-foreground shadow-sm backdrop-blur-sm',

  // 提升卡片 - 更强阴影效果,用于模态对话框/强调内容
  elevated: 'rounded-2xl border border-border bg-card text-card-foreground shadow-lg',

  // 交互卡片 - 带hover效果,用于可点击列表项
  interactive: 'rounded-2xl border border-border bg-card text-card-foreground shadow-sm transition-all duration-200 hover:shadow-md hover:border-primary/30 hover:-translate-y-0.5',

  // 轻量卡片 - 更淡的边框,用于辅助信息区域
  subtle: 'rounded-2xl border border-border/50 bg-card text-card-foreground shadow-sm',

  // 嵌入式卡片 - 极淡背景,用于卡片内的子卡片(保持层级关系)
  inset: 'rounded-xl border border-border/40 bg-card/40 text-card-foreground'
}

const cardClass = computed(() =>
  cn(variants[props.variant], props.class)
)
</script>
