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
  const csrfToken = useState('admin-csrf-token', () => '')
  const authenticated = useState('admin-authenticated', () => false)
  const loading = useState('admin-loading', () => false)
  const domains = useState<Domain[]>('admin-domains', () => [])
  const organizationalUnits = useState<OrganizationalUnit[]>('admin-ous', () => [])
  const users = useState<UserRecord[]>('admin-users', () => [])
  const groups = useState<GroupRecord[]>('admin-groups', () => [])
  const syncDomains = useState<SyncDomain[]>('admin-sync-domains', () => [])
  const oauthClients = useState<OAuthClient[]>('admin-oauth-clients', () => [])
  const tokenReady = computed(() => authenticated.value && csrfToken.value.length > 0)
  const { setStatus } = useAdminStatus()

  function resetData() {
    domains.value = []
    organizationalUnits.value = []
    users.value = []
    groups.value = []
    syncDomains.value = []
    oauthClients.value = []
  }

  function clearSession() {
    csrfToken.value = ''
    authenticated.value = false
    resetData()
  }

  async function restoreSession() {
    loading.value = true
    try {
      const response = await fetch('/api/admin/session', {
        credentials: 'same-origin',
      })
      if (!response.ok) {
        clearSession()
        return false
      }
      const payload = (await response.json()) as { csrf_token: string }
      csrfToken.value = payload.csrf_token
      authenticated.value = true
      await loadAll()
      return true
    } catch {
      clearSession()
      return false
    } finally {
      loading.value = false
    }
  }

  async function authenticateToken(token: string, showSuccess = true) {
    const trimmedToken = token.trim()
    if (!trimmedToken) {
      setStatus('请输入管理凭证', true)
      return false
    }

    loading.value = true
    try {
      const response = await fetch('/api/admin/session', {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
        },
        credentials: 'same-origin',
        body: JSON.stringify({ token: trimmedToken }),
      })

      if (!response.ok) {
        throw new Error(
          response.status === 401 || response.status === 403
            ? '管理凭证无效'
            : `${response.status} ${response.statusText}`,
        )
      }

      const payload = (await response.json()) as { csrf_token: string }
      csrfToken.value = payload.csrf_token
      authenticated.value = true
      await loadAll()
      if (showSuccess) {
        setStatus('已进入管理台')
      }
      return true
    } catch (error) {
      clearSession()
      setStatus(error instanceof Error ? error.message : '管理凭证无效', true)
      return false
    } finally {
      loading.value = false
    }
  }

  async function adminFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
    if (!tokenReady.value) {
      throw new Error('请先输入管理凭证')
    }

    const method = (init.method ?? 'GET').toUpperCase()
    const response = await fetch(path, {
      ...init,
      credentials: 'same-origin',
      headers: {
        ...(init.body ? { 'content-type': 'application/json' } : {}),
        ...(method === 'GET' || method === 'HEAD' || method === 'OPTIONS'
          ? {}
          : { 'x-adscope-csrf-token': csrfToken.value }),
        ...init.headers,
      },
    })

    if (!response.ok) {
      if (response.status === 401) {
        clearSession()
        throw new Error('管理会话已失效，请重新登录')
      }
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
      await loadAll()
    }, { successMessage: '数据已刷新' })
  }

  async function loadAll() {
    await Promise.all([
      loadDomains(),
      loadOus(),
      loadUsers(),
      loadGroups(),
      loadSyncDomains(),
      loadOAuthClients(),
    ])
  }

  async function logout() {
    if (!tokenReady.value) {
      return true
    }

    loading.value = true
    try {
      const response = await fetch('/api/admin/session', {
        method: 'DELETE',
        credentials: 'same-origin',
        headers: {
          'x-adscope-csrf-token': csrfToken.value,
        },
      })
      if (!response.ok) {
        throw new Error(`${response.status} ${response.statusText}`)
      }
      clearSession()
      setStatus('已退出管理台')
      return true
    } catch (error) {
      setStatus(error instanceof Error ? error.message : '退出失败', true)
      return false
    } finally {
      loading.value = false
    }
  }

  return {
    tokenReady,
    loading,
    domains,
    organizationalUnits,
    users,
    groups,
    syncDomains,
    oauthClients,
    restoreSession,
    authenticateToken,
    logout,
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
