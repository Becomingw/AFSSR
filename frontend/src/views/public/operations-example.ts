import type { MessageKey } from '@/i18n/messages'

export const OPERATIONS_BEAT_MS = 1200
export const operationSteps = [
  { id: 'report', icon: 'message', title: 'site.home.operations.report', description: 'site.home.operations.reportDescription', result: 'site.home.operations.reportResult', tools: [
    { name: 'afssr_feedback_list', label: 'site.home.operations.findIssue' },
    { name: 'afssr_feedback_get', label: 'site.home.operations.readIssue' },
    { name: 'afssr_identity', label: 'site.home.operations.checkIdentity' },
  ] },
  { id: 'diagnose', icon: 'search', title: 'site.home.operations.diagnose', description: 'site.home.operations.diagnoseDescription', result: 'site.home.operations.diagnoseResult', tools: [
    { name: 'afssr_feedback_get', label: 'site.home.operations.readIssue' },
    { name: 'afssr_request_diagnostics', label: 'site.home.operations.checkRequest' },
    { name: 'afssr_providers_health', label: 'site.home.operations.checkProvider' },
  ] },
  { id: 'repair', icon: 'wrench', title: 'site.home.operations.repair', description: 'site.home.operations.repairDescription', result: 'site.home.operations.repairResult', tools: [
    { name: 'afssr_providers_update', label: 'site.home.operations.previewChange' },
    { name: 'afssr_providers_update', label: 'site.home.operations.confirmChange' },
    { name: 'afssr_monitor_audit', label: 'site.home.operations.readAudit' },
  ] },
  { id: 'resolve', icon: 'check', title: 'site.home.operations.resolve', description: 'site.home.operations.resolveDescription', result: 'site.home.operations.resolveResult', tools: [
    { name: 'afssr_request_diagnostics', label: 'site.home.operations.verifyResult' },
    { name: 'afssr_feedback_command', label: 'site.home.operations.replyIssue' },
    { name: 'afssr_feedback_command', label: 'site.home.operations.resolveIssue' },
  ] },
] as const satisfies ReadonlyArray<{
  id: string
  icon: string
  title: MessageKey
  description: MessageKey
  result: MessageKey
  tools: ReadonlyArray<{ name: string, label: MessageKey }>
}>
