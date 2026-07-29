import type {
  Domain,
  GroupRecord,
  OrganizationalUnit,
  SyncDomain,
  UserRecord,
} from '~/types/admin'

interface AdminRequestOptions extends RequestInit {
  successMessage?: string
}

export function useAdminApi() {
  const managementToken = useState('admin-management-token', () => '')
  const loading = useState('admin-loading', () => false)
  const domains = useState<Domain[]>('admin-domains', () => [])
  const organizationalUnits = useState<OrganizationalUnit[]>('admin-ous', () => [])
  const users = useState<UserRecord[]>('admin-users', () => [])
  const groups = useState<GroupRecord[]>('admin-groups', () => [])
  const syncDomains = useState<SyncDomain[]>('admin-sync-domains', () => [])
  const tokenReady = computed(() => managementToken.value.trim().length > 0)
  const { setStatus } = useAdminStatus()

  function loadToken() {
    if (!import.meta.client) {
      return
    }
    managementToken.value = window.localStorage.getItem('adss.managementToken') ?? ''
  }

  function rememberToken(showMessage = true) {
    if (!import.meta.client) {
      return
    }
    window.localStorage.setItem('adss.managementToken', managementToken.value.trim())
    if (showMessage) {
      setStatus('管理凭证已保存到当前浏览器')
    }
  }

  function clearToken() {
    managementToken.value = ''
    domains.value = []
    organizationalUnits.value = []
    users.value = []
    groups.value = []
    syncDomains.value = []
    if (import.meta.client) {
      window.localStorage.removeItem('adss.managementToken')
    }
    setStatus('管理凭证已清除')
  }

  async function adminFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
    if (!tokenReady.value) {
      throw new Error('请先输入管理凭证')
    }

    const response = await fetch(path, {
      ...init,
      headers: {
        authorization: `Bearer ${managementToken.value.trim()}`,
        ...(init.body ? { 'content-type': 'application/json' } : {}),
        ...init.headers,
      },
    })

    if (!response.ok) {
      throw new Error(`${response.status} ${response.statusText}`)
    }

    if (response.status === 204) {
      return undefined as T
    }

    return (await response.json()) as T
  }

  async function runAction(action: () => Promise<void>, options: AdminRequestOptions = {}) {
    loading.value = true
    try {
      await action()
      if (options.successMessage) {
        setStatus(options.successMessage)
      }
    } catch (error) {
      setStatus(error instanceof Error ? error.message : '请求失败', true)
    } finally {
      loading.value = false
    }
  }

  async function loadDomains() {
    const response = await adminFetch<{ domains: Domain[] }>('/api/admin/domains')
    domains.value = response.domains
  }

  async function loadOus() {
    const response = await adminFetch<{ organizational_units: OrganizationalUnit[] }>(
      '/api/admin/ous/tree',
    )
    organizationalUnits.value = response.organizational_units
  }

  async function loadUsers() {
    const response = await adminFetch<{ users: UserRecord[] }>('/api/admin/users')
    users.value = response.users
  }

  async function loadGroups() {
    const response = await adminFetch<{ groups: GroupRecord[] }>('/api/admin/groups')
    groups.value = response.groups
  }

  async function loadSyncDomains() {
    const response = await adminFetch<{ domains: SyncDomain[] }>('/api/admin/sync/domains')
    syncDomains.value = response.domains
  }

  async function refreshDirectory() {
    await runAction(async () => {
      await Promise.all([loadOus(), loadUsers(), loadGroups(), loadSyncDomains()])
    }, { successMessage: '目录数据已刷新' })
  }

  async function refreshAll() {
    await runAction(async () => {
      await Promise.all([loadDomains(), loadOus(), loadUsers(), loadGroups(), loadSyncDomains()])
    }, { successMessage: '数据已刷新' })
  }

  return {
    managementToken,
    tokenReady,
    loading,
    domains,
    organizationalUnits,
    users,
    groups,
    syncDomains,
    loadToken,
    rememberToken,
    clearToken,
    adminFetch,
    runAction,
    loadDomains,
    loadOus,
    loadUsers,
    loadGroups,
    loadSyncDomains,
    refreshDirectory,
    refreshAll,
  }
}
