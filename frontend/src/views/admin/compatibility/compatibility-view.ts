import {
  runSummary,
  type CompatibilityCase,
  type CompatibilityProfile,
  type CompatibilityRun,
  type CompatibilitySnapshot,
} from '@/api/compatibility'

/** 离线合成实际执行的 9 个场景，顺序即矩阵列顺序。 */
export const EXECUTED_CASES = [
  'basic_request', 'tool_definition', 'tool_return', 'multi_turn_tools', 'long_history',
  'image', 'sync_response', 'tool_response', 'stream',
] as const

const CASE_LABELS: Record<string, string> = {
  basic_request: '基础请求', tool_definition: '工具定义', tool_return: '工具返回',
  multi_turn_tools: '多轮工具', long_history: '长历史（64 条消息）', image: '图片载体',
  sync_response: '同步响应', tool_response: '工具调用响应', stream: '文本 SSE',
  cancel: '取消', compression: '压缩与恢复', real_client: '真实客户端', real_model: '真实模型',
}
const CASE_SHORT: Record<string, string> = {
  basic_request: '基础', tool_definition: '工具定义', tool_return: '工具返回', multi_turn_tools: '多轮',
  long_history: '长历史', image: '图片', sync_response: '同步', tool_response: '工具响应', stream: 'SSE',
}
export const caseLabel = (value: string) => CASE_LABELS[value] ?? '未知项目'
export const caseShortLabel = (value: string) => CASE_SHORT[value] ?? caseLabel(value)

export type Tone = 'success' | 'danger' | 'warn' | 'muted'

/** 单个场景的状态展示；未知状态永远不会显示为通过。 */
export function caseTone(status: string | undefined): Tone {
  if (status === 'passed') return 'success'
  if (status === 'failed') return 'danger'
  if (status === 'unsupported') return 'warn'
  return 'muted'
}

const TONE_TEXT: Record<Tone, string> = {
  success: 'text-emerald-700 dark:text-emerald-400',
  danger: 'text-red-600 dark:text-red-400',
  warn: 'text-amber-700 dark:text-amber-400',
  muted: 'text-muted-foreground',
}
const TONE_PILL: Record<Tone, string> = {
  success: 'bg-emerald-500/10 text-emerald-700 ring-emerald-500/25 dark:text-emerald-400',
  danger: 'bg-red-500/10 text-red-600 ring-red-500/25 dark:text-red-400',
  warn: 'bg-amber-500/10 text-amber-700 ring-amber-500/25 dark:text-amber-400',
  muted: 'bg-muted text-muted-foreground ring-border',
}
const TONE_DOT: Record<Tone, string> = {
  success: 'bg-emerald-500',
  danger: 'bg-red-500',
  warn: 'bg-amber-500',
  muted: 'bg-muted-foreground/30',
}
export const toneText = (tone: Tone) => TONE_TEXT[tone]
export const tonePill = (tone: Tone) => TONE_PILL[tone]
export const toneDot = (tone: Tone) => TONE_DOT[tone]

export type ProfileState = 'passed' | 'failed' | 'unsupported' | 'unknown' | 'not_tested' | 'none'

export interface ProfileOverview {
  profile: CompatibilityProfile
  /** 最近一次合成记录，没有则为 undefined */
  latest?: { run: CompatibilityRun, stale: boolean }
  state: ProfileState
  /** 最近记录属于过期指纹（或从未运行） */
  needsRun: boolean
}

const STATE_LABEL: Record<ProfileState, string> = {
  passed: '合成通过', failed: '合成失败', unsupported: '部分不支持', unknown: '未知（非通过）',
  not_tested: '未完整测试', none: '未运行',
}
export const stateLabel = (state: ProfileState) => STATE_LABEL[state]
export function stateTone(state: ProfileState): Tone {
  if (state === 'passed') return 'success'
  if (state === 'failed') return 'danger'
  if (state === 'unsupported' || state === 'unknown') return 'warn'
  return 'muted'
}

/** 每个配置对应的最近记录（运行列表按时间倒序）。 */
export function overviewProfiles(snapshot: CompatibilitySnapshot | null): ProfileOverview[] {
  if (!snapshot) return []
  const latestByProfile = new Map<string, { run: CompatibilityRun, stale: boolean }>()
  for (const item of snapshot.runs) {
    if (!latestByProfile.has(item.run.profile_id)) latestByProfile.set(item.run.profile_id, item)
  }
  return snapshot.profiles.map((profile) => {
    const latest = latestByProfile.get(profile.id)
    const state: ProfileState = !latest
      ? 'none'
      : latest.run.evidence === 'offline_synthetic' ? (runSummary(latest.run.cases) as ProfileState) : 'unknown'
    return { profile, latest, state, needsRun: !latest || latest.stale !== false }
  })
}

export interface Kpis { total: number, passed: number, failed: number, needsRun: number }
export function kpis(overview: ProfileOverview[]): Kpis {
  return {
    total: overview.length,
    passed: overview.filter(item => item.state === 'passed' && !item.needsRun).length,
    failed: overview.filter(item => item.state === 'failed').length,
    needsRun: overview.filter(item => item.needsRun).length,
  }
}

const STATE_RANK: Record<ProfileState, number> = {
  failed: 0, unknown: 1, unsupported: 2, none: 3, not_tested: 4, passed: 5,
}
/** 失败与待处理的配置排在前面，其余保持原有顺序。 */
export function sortByAttention(items: ProfileOverview[]): ProfileOverview[] {
  return items
    .map((item, index) => ({ item, index }))
    .sort((a, b) => (STATE_RANK[a.item.state] - STATE_RANK[b.item.state]) || (a.index - b.index))
    .map(entry => entry.item)
}

export function caseCounts(cases: CompatibilityCase[]) {
  const counts = { passed: 0, failed: 0, unsupported: 0, other: 0 }
  for (const item of cases.filter(c => !['real_client', 'real_model', 'cancel', 'compression'].includes(c.case))) {
    if (item.status === 'passed') counts.passed++
    else if (item.status === 'failed') counts.failed++
    else if (item.status === 'unsupported') counts.unsupported++
    else counts.other++
  }
  return counts
}

export function caseStatus(run: CompatibilityRun | undefined, name: string): string | undefined {
  return run?.cases.find(item => item.case === name)?.status
}

export function relativeTime(seconds: number, nowMs = Date.now()): string {
  if (!Number.isFinite(seconds)) return '未知时间'
  const diff = Math.max(0, Math.floor(nowMs / 1000 - seconds))
  if (diff < 60) return '刚刚'
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`
  return new Date(seconds * 1000).toLocaleDateString()
}
