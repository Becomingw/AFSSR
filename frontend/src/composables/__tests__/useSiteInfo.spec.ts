import { beforeEach, describe, expect, it, vi } from 'vitest'

const apiClientMocks = vi.hoisted(() => ({
  get: vi.fn(),
}))

vi.mock('@/api/client', () => ({
  default: apiClientMocks,
}))

describe('useSiteInfo', () => {
  beforeEach(() => {
    vi.resetModules()
    apiClientMocks.get.mockReset()
    document.title = ''
  })

  it('loads public site info', async () => {
    apiClientMocks.get.mockResolvedValue({
      data: {
        site_name: 'Custom Aether',
        site_subtitle: 'Gateway',
      },
    })

    const { useSiteInfo } = await import('../useSiteInfo')
    const { siteName, siteSubtitle, siteInfoLoaded, refreshSiteInfo } = useSiteInfo()
    await refreshSiteInfo()

    expect(siteName.value).toBe('Custom Aether')
    expect(siteSubtitle.value).toBe('Gateway')
    expect(siteInfoLoaded.value).toBe(true)
    expect(document.title).toBe('Custom Aether')
  })

  it('keeps default site text hidden until public site info resolves', async () => {
    let resolveRequest: (value: { data: { site_name: string; site_subtitle: string } }) => void = () => {}
    apiClientMocks.get.mockReturnValue(new Promise((resolve) => {
      resolveRequest = resolve
    }))

    const { useSiteInfo } = await import('../useSiteInfo')
    const { siteName, siteSubtitle, siteInfoLoaded } = useSiteInfo()

    expect(siteName.value).toBe('')
    expect(siteSubtitle.value).toBe('')
    expect(siteInfoLoaded.value).toBe(false)
    expect(document.title).toBe('')

    resolveRequest({
      data: {
        site_name: 'Configured Aether',
        site_subtitle: 'Configured Gateway',
      },
    })
    await new Promise(resolve => setTimeout(resolve, 0))

    expect(siteName.value).toBe('Configured Aether')
    expect(siteSubtitle.value).toBe('Configured Gateway')
    expect(siteInfoLoaded.value).toBe(true)
    expect(document.title).toBe('Configured Aether')
  })

  it.each(['Aether', 'AFSSR', 'Arca'])('rebrands the legacy %s default without changing custom names', async (name) => {
    apiClientMocks.get.mockResolvedValue({ data: { site_name: name, site_subtitle: 'AI Gateway' } })
    const { useSiteInfo } = await import('../useSiteInfo')
    const { siteName, siteSubtitle, refreshSiteInfo } = useSiteInfo()
    await refreshSiteInfo()
    expect(siteName.value).toBe('Arca')
    expect(siteSubtitle.value).toBe('智枢 · AI Gateway')
    expect(document.title).toBe('Arca')
  })

  it('preserves a custom subtitle when replacing a legacy default name', async () => {
    apiClientMocks.get.mockResolvedValue({ data: { site_name: ' Aether ', site_subtitle: ' Team Gateway ' } })
    const { useSiteInfo } = await import('../useSiteInfo')
    const { siteName, siteSubtitle, refreshSiteInfo } = useSiteInfo()
    await refreshSiteInfo()
    expect(siteName.value).toBe('Arca')
    expect(siteSubtitle.value).toBe('Team Gateway')
  })

  it('uses Arca defaults only after public site info fails', async () => {
    apiClientMocks.get.mockRejectedValue(new Error('network unavailable'))

    const { useSiteInfo } = await import('../useSiteInfo')
    const { siteName, siteSubtitle, siteInfoLoaded, refreshSiteInfo } = useSiteInfo()

    expect(siteName.value).toBe('')
    expect(siteSubtitle.value).toBe('')

    await refreshSiteInfo()

    expect(siteName.value).toBe('Arca')
    expect(siteSubtitle.value).toBe('智枢 · AI Gateway')
    expect(siteInfoLoaded.value).toBe(true)
    expect(document.title).toBe('Arca')
  })
})
