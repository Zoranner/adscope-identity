import { useNotification } from '@stellar/ui'
import type {
  CreateOAuthClientRequest,
  Domain,
  DomainAttributeDefaults,
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

function requestErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback
}

export function useAdminApi() {
  const notifications = useNotification()
  const csrfToken = useState('admin-csrf-token', () => '')
  const authenticated = useState('admin-authenticated', () => false)
  const pending = useState('admin-pending', () => 0)
  const domains = useState<Domain[]>('admin-domains', () => [])
  const domainAttributeDefaults = useState<DomainAttributeDefaults>(
    'admin-domain-attribute-defaults',
    () => ({ employee_id_attribute: '', managed_group_id_attribute: '' }),
  )
  const organizationalUnits = useState<OrganizationalUnit[]>('admin-ous', () => [])
  const users = useState<UserRecord[]>('admin-users', () => [])
  const groups = useState<GroupRecord[]>('admin-groups', () => [])
  const syncDomains = useState<SyncDomain[]>('admin-sync-domains', () => [])
  const oauthClients = useState<OAuthClient[]>('admin-oauth-clients', () => [])
  const ousLoading = useState('admin-ous-loading', () => false)
  const usersLoading = useState('admin-users-loading', () => false)
  const groupsLoading = useState('admin-groups-loading', () => false)
  const domainsLoading = useState('admin-domains-loading', () => false)
  const syncLoading = useState('admin-sync-loading', () => false)
  const clientsLoading = useState('admin-clients-loading', () => false)
  const tokenReady = computed(() => authenticated.value && csrfToken.value.length > 0)
  const directoryLoading = computed(
    () => ousLoading.value || usersLoading.value || groupsLoading.value,
  )

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

  async function trackRequest<T>(request: () => Promise<T>): Promise<T> {
    pending.value += 1
    try {
      return await request()
    } finally {
      pending.value -= 1
    }
  }

  async function adminFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
    if (!tokenReady.value) {
      throw new Error('请先输入管理凭证')
    }

    const method = (init.method ?? 'GET').toUpperCase()
    return await trackRequest(async () => {
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
          throw new Error('管理会话已失效，请重新输入管理凭证')
        }
        throw new Error(`${response.status} ${response.statusText}`)
      }

      if (response.status === 204) {
        return undefined as T
      }

      return (await response.json()) as T
    })
  }

  async function runAction(action: () => Promise<void>, options: AdminRequestOptions = {}) {
    try {
      await action()
      if (options.successMessage) {
        notifications.success(options.successMessage)
      }
    } catch (error) {
      notifications.error(requestErrorMessage(error, '请求失败'))
    }
  }

  async function restoreSession() {
    try {
      const response = await trackRequest(() =>
        fetch('/api/admin/session', { credentials: 'same-origin' }),
      )
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
    }
  }

  async function authenticateToken(token: string, showSuccess = true) {
    const trimmedToken = token.trim()
    if (!trimmedToken) {
      notifications.error('请输入管理凭证')
      return false
    }

    try {
      const response = await trackRequest(() =>
        fetch('/api/admin/session', {
          method: 'POST',
          headers: {
            'content-type': 'application/json',
          },
          credentials: 'same-origin',
          body: JSON.stringify({ token: trimmedToken }),
        }),
      )

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
        notifications.success('已进入管理控制台')
      }
      return true
    } catch (error) {
      clearSession()
      notifications.error(requestErrorMessage(error, '管理凭证无效'))
      return false
    }
  }

  async function loadDomains() {
    domainsLoading.value = true
    try {
      const response = await adminFetch<{
        domains: Domain[]
        attribute_defaults: DomainAttributeDefaults
      }>('/api/admin/domains')
      domains.value = response.domains
      domainAttributeDefaults.value = response.attribute_defaults
    } finally {
      domainsLoading.value = false
    }
  }

  async function loadOus() {
    ousLoading.value = true
    try {
      const response = await adminFetch<{ organizational_units: OrganizationalUnit[] }>(
        '/api/admin/ous/tree',
      )
      organizationalUnits.value = response.organizational_units
    } finally {
      ousLoading.value = false
    }
  }

  async function loadUsers() {
    usersLoading.value = true
    try {
      const response = await adminFetch<{ users: UserRecord[] }>('/api/admin/users')
      users.value = response.users
    } finally {
      usersLoading.value = false
    }
  }

  async function loadGroups() {
    groupsLoading.value = true
    try {
      const response = await adminFetch<{ groups: GroupRecord[] }>('/api/admin/groups')
      groups.value = response.groups
    } finally {
      groupsLoading.value = false
    }
  }

  async function loadSyncDomains() {
    syncLoading.value = true
    try {
      const response = await adminFetch<{ domains: SyncDomain[] }>('/api/admin/sync/domains')
      syncDomains.value = response.domains
    } finally {
      syncLoading.value = false
    }
  }

  async function loadOAuthClients() {
    clientsLoading.value = true
    try {
      const response = await adminFetch<{ clients: OAuthClient[] }>('/api/admin/oauth-clients')
      oauthClients.value = response.clients
    } finally {
      clientsLoading.value = false
    }
  }

  async function loadDirectory() {
    await Promise.all([loadOus(), loadUsers(), loadGroups()])
  }

  async function loadAll() {
    await Promise.all([loadDirectory(), loadDomains(), loadSyncDomains(), loadOAuthClients()])
  }

  async function refreshDirectory() {
    await runAction(async () => {
      await Promise.all([loadOus(), loadUsers(), loadGroups(), loadSyncDomains()])
    }, { successMessage: '目录数据已刷新' })
  }

  async function refreshAll() {
    await runAction(
      async () => {
        await loadAll()
      },
      { successMessage: '数据已刷新' },
    )
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

  async function logout() {
    if (!authenticated.value) {
      return true
    }
    try {
      await trackRequest(() =>
        fetch('/api/admin/session', {
          method: 'DELETE',
          credentials: 'same-origin',
          headers: { 'x-adscope-csrf-token': csrfToken.value },
        }),
      )
    } catch {
      // 本地会话必须清理，即使服务端不可达。
    } finally {
      clearSession()
    }
    notifications.success('已退出管理控制台')
    return true
  }

  return {
    csrfToken,
    authenticated,
    tokenReady,
    pending,
    domains,
    domainAttributeDefaults,
    organizationalUnits,
    users,
    groups,
    syncDomains,
    oauthClients,
    ousLoading,
    usersLoading,
    groupsLoading,
    domainsLoading,
    syncLoading,
    clientsLoading,
    directoryLoading,
    adminFetch,
    runAction,
    restoreSession,
    authenticateToken,
    loadDomains,
    loadOus,
    loadUsers,
    loadGroups,
    loadSyncDomains,
    loadOAuthClients,
    loadDirectory,
    loadAll,
    refreshDirectory,
    refreshAll,
    createOAuthClient,
    updateOAuthClient,
    deleteOAuthClient,
    regenerateOAuthClientSecret,
    clearSession,
    logout,
  }
}
