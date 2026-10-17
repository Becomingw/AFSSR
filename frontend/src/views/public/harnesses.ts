export const HARNESS_ROTATION_MS = 4800

export const harnesses = [
  { id: 'codex', name: 'Codex', protocol: 'OpenAI Responses', logo: '/openai.svg', monochrome: true },
  { id: 'claude', name: 'Claude Code', protocol: 'Claude Messages', logo: '/claude-color.svg', monochrome: false },
  { id: 'opencode', name: 'OpenCode', protocol: 'OpenAI / Claude', logo: '/opencode.svg', monochrome: true },
  { id: 'gemini', name: 'Gemini CLI', protocol: 'Gemini API', logo: '/gemini-color.svg', monochrome: false },
  { id: 'cline', name: 'Cline', protocol: 'OpenAI Chat Completions', logo: '/cline.svg', monochrome: true },
] as const

export function wrapHarnessIndex(index: number): number {
  return ((index % harnesses.length) + harnesses.length) % harnesses.length
}
