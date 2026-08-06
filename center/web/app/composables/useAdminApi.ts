import type {
  CreateOAuthClientRequest,
  Domain,
  GroupRecord,
  OAuthClient,
  OAuthClientCreateResponse,
  OAuthClientSecretResponse,
  OAuthClientUpdateResponse,
  OrganizationalUnit,
  SyncDomain,
  UpdateOAuthClientRequest,
  UserRecord,
} from '~/types/admin'

interface AdminRequestOptions extends RequestInit {
  successMessage?: string
}

export function useAdminApi() {
  const managementToken = useState('admin-management-token', () => '')
  const authenticated = useState('admin-authenticated', () => false)
  const loading = useState('admin-loading', () => false)
  const domains = useState<Domain[]>('admin-domains', () => [])
  const organizationalUnits = useState<OrganizationalUnit[]>('admin-ous', () => [])
  const users = useState<UserRecord[]>('admin-users', () => [])
  const groups = useState<GroupRecord[]>('admin-groups', () => [])
  const syncDomains = useState<SyncDomain[]>('admin-sync-domains', () => [])
  const oauthClients = useState<OAuthClient[]>('admin-oauth-clients', () => [])
  const tokenReady = computed(
    () => authenticated.value && managementToken.value.trim().length > 0,
  )
  const { setStatus } = useAdminStatus()

  function resetData() {
    domains.value = []
    organizationalUnits.value = []
    users.value = []
    groups.value = []
    syncDomains.value = []
    oauthClients.value = []
  }

  function loadToken(): string {
    if (!import.meta.client) {
      return ''
    }
    const storedToken = window.localStorage.getItem('adss.managementToken') ?? ''
    managementToken.value = storedToken
    authenticated.value = false
    return storedToken
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
    authenticated.value = false
    resetData()
    if (import.meta.client) {
      window.localStorage.removeItem('adss.managementToken')
    }
    setStatus('管理凭证已清除')
  }

  async function authenticateToken(token: string, showSuccess = true) {
    const trimmedToken = token.trim()
    if (!trimmedToken) {
      setStatus('请输入管理凭证', true)
      return
    }

    loading.value = true
    try {
      const response = await fetch('/api/admin/domains', {
        headers: {
          authorization: `Bearer ${trimmedToken}`,
        },
      })

      if (!response.ok) {
        throw new Error(
          response.status === 401 || response.status === 403
            ? '管理凭证无效'
            : `${response.status} ${response.statusText}`,
        )
      }

      const payload = (await response.json()) as { domains: Domain[] }
      managementToken.value = trimmedToken
      authenticated.value = true
      domains.value = payload.domains
      if (import.meta.client) {
        window.localStorage.setItem('adss.managementToken', trimmedToken)
      }
      await Promise.all([
        loadOus(),
        loadUsers(),
        loadGroups(),
        loadSyncDomains(),
        loadOAuthClients(),
      ])
      if (showSuccess) {
        setStatus('已进入管理台')
      }
    } catch (error) {
      managementToken.value = ''
      authenticated.value = false
      resetData()
      if (import.meta.client) {
        window.localStorage.removeItem('adss.managementToken')
      }
      setStatus(error instanceof Error ? error.message : '管理凭证无效', true)
    } finally {
      loading.value = false
    }
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

  async function loadOAuthClients() {
    const response = await adminFetch<{ clients: OAuthClient[] }>('/api/admin/oauth-clients')
    oauthClients.value = response.clients
  }

  async function createOAuthClient(
    request: CreateOAuthClientRequest,
  ): Promise<OAuthClientCreateResponse> {
    const response = await adminFetch<OAuthClientCreateResponse>('/api/admin/oauth-clients', {
      method: 'POST',
      body: JSON.stringify(request),
    })
    oauthClients.value = [...oauthClients.value, response.client]
    return response
  }

  async function updateOAuthClient(
    clientId: string,
    request: UpdateOAuthClientRequest,
  ): Promise<OAuthClientUpdateResponse> {
    const response = await adminFetch<OAuthClientUpdateResponse>(
      `/api/admin/oauth-clients/${encodeURIComponent(clientId)}`,
      {
        method: 'PATCH',
        body: JSON.stringify(request),
      },
    )
    oauthClients.value = oauthClients.value.map((client) =>
      client.client_id === response.client_id ? response : client,
    )
    return response
  }

  async function deleteOAuthClient(clientId: string): Promise<void> {
    await adminFetch<void>(`/api/admin/oauth-clients/${encodeURIComponent(clientId)}`, {
      method: 'DELETE',
    })
    oauthClients.value = oauthClients.value.filter((client) => client.client_id !== clientId)
  }

  async function regenerateOAuthClientSecret(
    clientId: string,
  ): Promise<OAuthClientSecretResponse> {
    return await adminFetch<OAuthClientSecretResponse>(
      `/api/admin/oauth-clients/${encodeURIComponent(clientId)}/secret`,
      { method: 'POST' },
    )
  }

  async function refreshDirectory() {
    await runAction(async () => {
      await Promise.all([loadOus(), loadUsers(), loadGroups(), loadSyncDomains()])
    }, { successMessage: '目录数据已刷新' })
  }

  async function refreshAll() {
    await runAction(async () => {
      await Promise.all([
        loadDomains(),
        loadOus(),
        loadUsers(),
        loadGroups(),
        loadSyncDomains(),
        loadOAuthClients(),
      ])
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
    oauthClients,
    loadToken,
    rememberToken,
    clearToken,
    authenticateToken,
    adminFetch,
    runAction,
    loadDomains,
    loadOus,
    loadUsers,
    loadGroups,
    loadSyncDomains,
    loadOAuthClients,
    createOAuthClient,
    updateOAuthClient,
    deleteOAuthClient,
    regenerateOAuthClientSecret,
    refreshDirectory,
    refreshAll,
  }
}
