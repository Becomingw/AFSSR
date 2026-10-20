import { readonly, ref, watch } from 'vue'
import apiClient from '@/api/client'
import { ARCA_NAME, ARCA_SUBTITLE } from '@/constants/arcaBrand'

interface SiteInfo {
  site_name: string
  site_subtitle: string
}

const DEFAULT_SITE_INFO: SiteInfo = {
  site_name: ARCA_NAME,
  site_subtitle: ARCA_SUBTITLE,
}

// 模块级缓存，所有组件共享同一份数据
const siteName = ref('')
const siteSubtitle = ref('')
const loaded = ref(false)
let fetchPromise: Promise<void> | null = null

function normalizeSiteInfo(data: Partial<SiteInfo> | null | undefined): SiteInfo {
  const name = data?.site_name?.trim()
  const subtitle = data?.site_subtitle?.trim()
  // Untouched backend defaults (site_name + 'AI Gateway' subtitle) map to the
  // Arca brand defaults; 'Arca' is the current backend default site_name.
  const legacyDefault = name === 'Aether' || name === 'AFSSR' || name === ARCA_NAME
  return {
    site_name: !name || legacyDefault ? DEFAULT_SITE_INFO.site_name : name,
    site_subtitle: !subtitle || (legacyDefault && subtitle === 'AI Gateway')
      ? DEFAULT_SITE_INFO.site_subtitle
      : subtitle,
  }
}

function applySiteInfo(data: Partial<SiteInfo> | null | undefined): void {
  const normalized = normalizeSiteInfo(data)
  siteName.value = normalized.site_name
  siteSubtitle.value = normalized.site_subtitle
}

async function fetchSiteInfo() {
  try {
    const response = await apiClient.get<SiteInfo>('/api/public/site-info')
    applySiteInfo(response.data)
  } catch {
    // 加载失败时才使用默认站点信息，避免配置加载前闪出默认品牌文案
    if (!siteName.value || !siteSubtitle.value) {
      applySiteInfo(DEFAULT_SITE_INFO)
    }
    fetchPromise = null
  } finally {
    loaded.value = true
  }
}

async function refreshSiteInfo() {
  fetchPromise = null
  loaded.value = false
  fetchPromise = fetchSiteInfo()
  await fetchPromise
}

export function useSiteInfo() {
  if (!loaded.value && !fetchPromise) {
    fetchPromise = fetchSiteInfo()
  }
  return { siteName, siteSubtitle, siteInfoLoaded: readonly(loaded), refreshSiteInfo }
}

// 站点名称变化时同步更新 document.title
watch(siteName, (name) => {
  if (name) {
    document.title = name
  }
}, { immediate: true })
