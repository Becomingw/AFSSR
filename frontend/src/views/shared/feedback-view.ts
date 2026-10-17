export const feedbackStatuses = [{ value: '', label: '全部' }, { value: 'open', label: '待处理' }, { value: 'resolved', label: '已解决' }, { value: 'closed', label: '已关闭' }]
export const feedbackCategories = [{ value: 'bug', label: '程序问题' }, { value: 'model', label: '模型调用' }, { value: 'access', label: '账号与权益' }, { value: 'suggestion', label: '功能建议' }, { value: 'other', label: '其他' }]
export function feedbackStatusLabel(value: string) { return feedbackStatuses.find(item => item.value === value)?.label || value }
export function feedbackCategoryLabel(value: string) { return feedbackCategories.find(item => item.value === value)?.label || value }
export function feedbackDate(value: number) { return new Date(value * 1000).toLocaleString() }
export function feedbackShortId(id: string) { return id.length > 12 ? `${id.slice(0, 4)}…${id.slice(-6)}` : id }
