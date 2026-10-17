/**
 * 保存密钥模型权限时，需要新关联到提供商的模型：
 * 已选中、且还不是该提供商关联模型的名称（去重、去空白，保持选择顺序）。
 * 只增不删：取消勾选的模型不会解除已有关联。
 */
export function modelsNeedingProviderLink(
  selectedModels: readonly string[],
  linkedProviderModelNames: Iterable<string>,
): string[] {
  const linked = new Set(linkedProviderModelNames)
  const seen = new Set<string>()
  const result: string[] = []
  for (const raw of selectedModels) {
    const name = raw.trim()
    if (!name || linked.has(name) || seen.has(name)) continue
    seen.add(name)
    result.push(name)
  }
  return result
}
