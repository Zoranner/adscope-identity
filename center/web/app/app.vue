<script setup lang="ts">
import {
  Building2,
  CheckCircle2,
  Database,
  FolderTree,
  KeyRound,
  RefreshCw,
  Save,
  ShieldCheck,
  Users,
  Workflow,
} from 'lucide-vue-next'

type ViewKey = 'directory' | 'domains' | 'sync'
type DirectoryFormKey = 'ou' | 'user' | 'group'
type UserStatus = 'active' | 'disabled'

interface Domain {
  id: string
  name: string
  enabled: boolean
  mirror_root_dn: string
  quarantine_ou_dn: string
  upn_suffix: string
  employee_id_attribute: string
  managed_group_id_attribute: string
  applied_directory_revision: number
  applied_credential_revision: number
}

interface OrganizationalUnit {
  id: string
  name: string
  parent_id: string | null
  changed_revision: number
}

interface UserRecord {
  employee_id: string
  username: string
  display_name: string
  email: string | null
  mobile: string | null
  telephone: string | null
  organizational_unit_id: string
  status: UserStatus
}

interface GroupRecord {
  id: string
  name: string
  organizational_unit_id: string
  member_employee_ids: string[]
  changed_revision: number
}

interface SyncDomain {
  domain_id: string
  enabled: boolean
  applied_directory_revision: number
  applied_credential_revision: number
  directory_lag: number
  credential_lag: number
}

interface DomainForm {
  id: string
  name: string
  enabled: boolean
  mirror_root_dn: string
  quarantine_ou_dn: string
  upn_suffix: string
  employee_id_attribute: string
  managed_group_id_attribute: string
  connector_key: string
}

interface OuForm {
  id: string
  name: string
  parent_id: string
}

interface UserForm {
  employee_id: string
  username: string
  display_name: string
  email: string
  mobile: string
  telephone: string
  organizational_unit_id: string
  status: UserStatus
  initial_password: string
  reset_password: string
}

interface GroupForm {
  id: string
  name: string
  organizational_unit_id: string
  member_employee_ids: string
}

interface OuTreeItem {
  ou: OrganizationalUnit
  depth: number
  userCount: number
  groupCount: number
}

const views: Array<{ key: ViewKey; label: string; icon: typeof Building2 }> = [
  { key: 'directory', label: '目录', icon: FolderTree },
  { key: 'domains', label: '域', icon: Building2 },
  { key: 'sync', label: '同步', icon: Workflow },
]
const defaultView = views[0]!

const activeView = ref<ViewKey>('directory')
const activeDirectoryForm = ref<DirectoryFormKey>('ou')
const managementToken = ref('')
const statusMessage = ref('')
const isError = ref(false)
const loading = ref(false)

const domains = ref<Domain[]>([])
const organizationalUnits = ref<OrganizationalUnit[]>([])
const users = ref<UserRecord[]>([])
const groups = ref<GroupRecord[]>([])
const syncDomains = ref<SyncDomain[]>([])

const selectedDomainId = ref<string | null>(null)
const selectedOuId = ref<string | null>(null)
const editingOuId = ref<string | null>(null)
const selectedUserId = ref<string | null>(null)
const selectedGroupId = ref<string | null>(null)

const domainForm = reactive<DomainForm>(blankDomainForm())
const ouForm = reactive<OuForm>(blankOuForm())
const userForm = reactive<UserForm>(blankUserForm())
const groupForm = reactive<GroupForm>(blankGroupForm())

const currentView = computed(() => views.find((view) => view.key === activeView.value) ?? defaultView)
const tokenReady = computed(() => managementToken.value.trim().length > 0)
const selectedOu = computed(
  () => organizationalUnits.value.find((ou) => ou.id === selectedOuId.value) ?? null,
)
const selectedOuUsers = computed(() =>
  selectedOuId.value
    ? users.value.filter((user) => user.organizational_unit_id === selectedOuId.value)
    : [],
)
const selectedOuGroups = computed(() =>
  selectedOuId.value
    ? groups.value.filter((group) => group.organizational_unit_id === selectedOuId.value)
    : [],
)
const ouTreeItems = computed<OuTreeItem[]>(() => flattenOus())
const activeDirectoryLag = computed(() =>
  syncDomains.value.reduce((sum, domain) => sum + domain.directory_lag, 0),
)
const activeCredentialLag = computed(() =>
  syncDomains.value.reduce((sum, domain) => sum + domain.credential_lag, 0),
)

onMounted(() => {
  managementToken.value = window.localStorage.getItem('adss.managementToken') ?? ''
  if (tokenReady.value) {
    void refreshAll()
  }
})

function blankDomainForm(): DomainForm {
  return {
    id: '',
    name: '',
    enabled: true,
    mirror_root_dn: '',
    quarantine_ou_dn: '',
    upn_suffix: '',
    employee_id_attribute: 'employeeID',
    managed_group_id_attribute: 'adminDescription',
    connector_key: '',
  }
}

function blankOuForm(): OuForm {
  return {
    id: '',
    name: '',
    parent_id: '',
  }
}

function blankUserForm(): UserForm {
  return {
    employee_id: '',
    username: '',
    display_name: '',
    email: '',
    mobile: '',
    telephone: '',
    organizational_unit_id: '',
    status: 'active',
    initial_password: '',
    reset_password: '',
  }
}

function blankGroupForm(): GroupForm {
  return {
    id: '',
    name: '',
    organizational_unit_id: '',
    member_employee_ids: '',
  }
}

function assignForm<T extends object>(target: T, source: T) {
  Object.assign(target, source)
}

function nullable(value: string): string | null {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function optionalParentId(value: string): string | null {
  return nullable(value)
}

function splitMembers(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
}

function sortOus(ous: OrganizationalUnit[]): OrganizationalUnit[] {
  return [...ous].sort((left, right) => {
    const byName = left.name.localeCompare(right.name, 'zh-Hans-CN')
    return byName === 0 ? left.id.localeCompare(right.id) : byName
  })
}

function flattenOus(): OuTreeItem[] {
  const byParent = new Map<string | null, OrganizationalUnit[]>()
  for (const ou of organizationalUnits.value) {
    const siblings = byParent.get(ou.parent_id) ?? []
    siblings.push(ou)
    byParent.set(ou.parent_id, siblings)
  }

  for (const [parentId, siblings] of byParent.entries()) {
    byParent.set(parentId, sortOus(siblings))
  }

  const visited = new Set<string>()
  const items: OuTreeItem[] = []
  const pushChildren = (parentId: string | null, depth: number) => {
    for (const ou of byParent.get(parentId) ?? []) {
      if (visited.has(ou.id)) {
        continue
      }
      visited.add(ou.id)
      items.push({
        ou,
        depth,
        userCount: countUsersInOu(ou.id),
        groupCount: countGroupsInOu(ou.id),
      })
      pushChildren(ou.id, depth + 1)
    }
  }

  pushChildren(null, 0)
  for (const ou of sortOus(organizationalUnits.value)) {
    if (!visited.has(ou.id)) {
      visited.add(ou.id)
      items.push({
        ou,
        depth: 0,
        userCount: countUsersInOu(ou.id),
        groupCount: countGroupsInOu(ou.id),
      })
      pushChildren(ou.id, 1)
    }
  }

  return items
}

function countUsersInOu(ouId: string): number {
  return users.value.filter((user) => user.organizational_unit_id === ouId).length
}

function countGroupsInOu(ouId: string): number {
  return groups.value.filter((group) => group.organizational_unit_id === ouId).length
}

function ouName(ouId: string | null | undefined): string {
  if (!ouId) {
    return '-'
  }
  return organizationalUnits.value.find((ou) => ou.id === ouId)?.name ?? ouId
}

function reconcileSelectedOu() {
  if (organizationalUnits.value.length === 0) {
    selectedOuId.value = null
    editingOuId.value = null
    assignForm(ouForm, blankOuForm())
    return
  }

  const current = organizationalUnits.value.find((ou) => ou.id === selectedOuId.value)
  if (current) {
    selectOu(current, false)
    return
  }

  selectOu(sortOus(organizationalUnits.value)[0]!, false)
}

function rememberToken() {
  window.localStorage.setItem('adss.managementToken', managementToken.value.trim())
  setStatus('管理凭证已保存到当前浏览器')
}

function clearToken() {
  managementToken.value = ''
  window.localStorage.removeItem('adss.managementToken')
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

async function runAction(action: () => Promise<void>, message: string) {
  loading.value = true
  try {
    await action()
    setStatus(message)
  } catch (error) {
    setStatus(error instanceof Error ? error.message : '请求失败', true)
  } finally {
    loading.value = false
  }
}

function setStatus(message: string, error = false) {
  statusMessage.value = message
  isError.value = error
}

async function refreshAll() {
  await runAction(async () => {
    await Promise.all([loadDomains(), loadOus(), loadUsers(), loadGroups(), loadSyncDomains()])
  }, '数据已刷新')
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
  reconcileSelectedOu()
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

function selectDomain(domain: Domain) {
  selectedDomainId.value = domain.id
  assignForm(domainForm, {
    id: domain.id,
    name: domain.name,
    enabled: domain.enabled,
    mirror_root_dn: domain.mirror_root_dn,
    quarantine_ou_dn: domain.quarantine_ou_dn,
    upn_suffix: domain.upn_suffix,
    employee_id_attribute: domain.employee_id_attribute,
    managed_group_id_attribute: domain.managed_group_id_attribute,
    connector_key: '',
  })
}

function newDomain() {
  selectedDomainId.value = null
  assignForm(domainForm, blankDomainForm())
}

async function saveDomain() {
  await runAction(async () => {
    const payload = {
      name: domainForm.name,
      enabled: domainForm.enabled,
      mirror_root_dn: domainForm.mirror_root_dn,
      quarantine_ou_dn: domainForm.quarantine_ou_dn,
      upn_suffix: domainForm.upn_suffix,
      employee_id_attribute: domainForm.employee_id_attribute,
      managed_group_id_attribute: domainForm.managed_group_id_attribute,
    }
    if (selectedDomainId.value) {
      await adminFetch<Domain>(`/api/admin/domains/${encodeURIComponent(selectedDomainId.value)}`, {
        method: 'PATCH',
        body: JSON.stringify(payload),
      })
    } else {
      await adminFetch<Domain>('/api/admin/domains', {
        method: 'POST',
        body: JSON.stringify({
          id: domainForm.id,
          connector_key: domainForm.connector_key,
          ...payload,
        }),
      })
    }
    await loadDomains()
    await loadSyncDomains()
  }, selectedDomainId.value ? '域配置已更新' : '域配置已创建')
}

function selectOu(ou: OrganizationalUnit, activate = true) {
  if (activate) {
    activeView.value = 'directory'
    activeDirectoryForm.value = 'ou'
  }
  selectedOuId.value = ou.id
  editingOuId.value = ou.id
  assignForm(ouForm, {
    id: ou.id,
    name: ou.name,
    parent_id: ou.parent_id ?? '',
  })
  if (activate) {
    selectedUserId.value = null
    selectedGroupId.value = null
  }
}

function newOu(parentId = selectedOuId.value) {
  activeView.value = 'directory'
  activeDirectoryForm.value = 'ou'
  editingOuId.value = null
  assignForm(ouForm, {
    ...blankOuForm(),
    parent_id: parentId ?? '',
  })
}

async function saveOu() {
  await runAction(async () => {
    const payload = {
      name: ouForm.name,
      parent_id: optionalParentId(ouForm.parent_id),
    }
    if (editingOuId.value) {
      await adminFetch(`/api/admin/ous/${encodeURIComponent(editingOuId.value)}`, {
        method: 'PATCH',
        body: JSON.stringify(payload),
      })
    } else {
      await adminFetch('/api/admin/ous', {
        method: 'POST',
        body: JSON.stringify({
          id: ouForm.id,
          ...payload,
        }),
      })
      selectedOuId.value = ouForm.id
    }
    await loadOus()
    await loadSyncDomains()
  }, editingOuId.value ? 'OU 已更新' : 'OU 已创建')
}

function selectUser(user: UserRecord) {
  activeView.value = 'directory'
  activeDirectoryForm.value = 'user'
  selectedUserId.value = user.employee_id
  selectedOuId.value = user.organizational_unit_id
  assignForm(userForm, {
    employee_id: user.employee_id,
    username: user.username,
    display_name: user.display_name,
    email: user.email ?? '',
    mobile: user.mobile ?? '',
    telephone: user.telephone ?? '',
    organizational_unit_id: user.organizational_unit_id,
    status: user.status,
    initial_password: '',
    reset_password: '',
  })
}

function newUser() {
  if (!selectedOuId.value) {
    setStatus('请先选择 OU', true)
    return
  }
  activeView.value = 'directory'
  activeDirectoryForm.value = 'user'
  selectedUserId.value = null
  assignForm(userForm, {
    ...blankUserForm(),
    organizational_unit_id: selectedOuId.value,
  })
}

async function saveUser() {
  await runAction(async () => {
    const payload = {
      username: userForm.username,
      display_name: userForm.display_name,
      email: nullable(userForm.email),
      mobile: nullable(userForm.mobile),
      telephone: nullable(userForm.telephone),
      organizational_unit_id: userForm.organizational_unit_id,
      status: userForm.status,
    }
    if (selectedUserId.value) {
      await adminFetch(`/api/admin/users/${encodeURIComponent(selectedUserId.value)}`, {
        method: 'PATCH',
        body: JSON.stringify(payload),
      })
    } else {
      await adminFetch('/api/admin/users', {
        method: 'POST',
        body: JSON.stringify({
          employee_id: userForm.employee_id,
          initial_password: userForm.initial_password,
          ...payload,
        }),
      })
    }
    await loadUsers()
    await loadSyncDomains()
    selectedOuId.value = userForm.organizational_unit_id
  }, selectedUserId.value ? '用户已更新' : '用户已创建')
}

async function setUserEnabled(enabled: boolean) {
  if (!selectedUserId.value) {
    setStatus('请先选择用户', true)
    return
  }

  await runAction(async () => {
    await adminFetch(
      `/api/admin/users/${encodeURIComponent(selectedUserId.value!)}/${enabled ? 'enable' : 'disable'}`,
      { method: 'POST' },
    )
    await loadUsers()
    await loadSyncDomains()
  }, enabled ? '用户已启用' : '用户已禁用')
}

async function resetUserPassword() {
  if (!selectedUserId.value) {
    setStatus('请先选择用户', true)
    return
  }

  await runAction(async () => {
    await adminFetch(`/api/admin/users/${encodeURIComponent(selectedUserId.value!)}/password-reset`, {
      method: 'POST',
      body: JSON.stringify({
        new_password: userForm.reset_password,
      }),
    })
    userForm.reset_password = ''
    await loadSyncDomains()
  }, '用户密码已重置')
}

function selectGroup(group: GroupRecord) {
  activeView.value = 'directory'
  activeDirectoryForm.value = 'group'
  selectedGroupId.value = group.id
  selectedOuId.value = group.organizational_unit_id
  assignForm(groupForm, {
    id: group.id,
    name: group.name,
    organizational_unit_id: group.organizational_unit_id,
    member_employee_ids: group.member_employee_ids.join(', '),
  })
}

function newGroup() {
  if (!selectedOuId.value) {
    setStatus('请先选择 OU', true)
    return
  }
  activeView.value = 'directory'
  activeDirectoryForm.value = 'group'
  selectedGroupId.value = null
  assignForm(groupForm, {
    ...blankGroupForm(),
    organizational_unit_id: selectedOuId.value,
  })
}

async function saveGroup() {
  await runAction(async () => {
    const payload = {
      name: groupForm.name,
      organizational_unit_id: groupForm.organizational_unit_id,
    }
    if (selectedGroupId.value) {
      await adminFetch(`/api/admin/groups/${encodeURIComponent(selectedGroupId.value)}`, {
        method: 'PATCH',
        body: JSON.stringify(payload),
      })
      await adminFetch(`/api/admin/groups/${encodeURIComponent(selectedGroupId.value)}/members`, {
        method: 'PUT',
        body: JSON.stringify({
          member_employee_ids: splitMembers(groupForm.member_employee_ids),
        }),
      })
    } else {
      await adminFetch('/api/admin/groups', {
        method: 'POST',
        body: JSON.stringify({
          id: groupForm.id,
          ...payload,
        }),
      })
      if (groupForm.member_employee_ids.trim().length > 0) {
        await adminFetch(`/api/admin/groups/${encodeURIComponent(groupForm.id)}/members`, {
          method: 'PUT',
          body: JSON.stringify({
            member_employee_ids: splitMembers(groupForm.member_employee_ids),
          }),
        })
      }
    }
    await loadGroups()
    await loadSyncDomains()
    selectedOuId.value = groupForm.organizational_unit_id
  }, selectedGroupId.value ? '组已更新' : '组已创建')
}
</script>

<template>
  <div class="app-shell">
    <header class="topbar">
      <div class="brand">
        <span class="brand-mark">
          <Database :size="22" />
        </span>
        <div>
          <h1 class="brand-title">ADSS Center</h1>
          <p class="brand-subtitle">中心事实源管理工作台</p>
        </div>
      </div>
      <div class="token-panel">
        <KeyRound :size="18" />
        <input v-model="managementToken" type="password" placeholder="管理凭证" />
        <button class="secondary-button" :disabled="!managementToken" @click="rememberToken">
          保存
        </button>
        <button class="icon-button" title="刷新" :disabled="loading || !tokenReady" @click="refreshAll">
          <RefreshCw :size="17" />
        </button>
        <button class="icon-button" title="清除凭证" @click="clearToken">
          <KeyRound :size="17" />
        </button>
      </div>
    </header>

    <div class="layout">
      <aside class="sidebar">
        <nav class="nav-list top-nav">
          <button
            v-for="view in views"
            :key="view.key"
            class="nav-button"
            :class="{ active: activeView === view.key }"
            @click="activeView = view.key"
          >
            <component :is="view.icon" :size="18" />
            {{ view.label }}
          </button>
        </nav>

        <div class="ou-sidebar">
          <div class="sidebar-section-header">
            <h3>OU 树</h3>
            <button class="secondary-button compact-button" @click="newOu(null)">新建</button>
          </div>
          <div v-if="ouTreeItems.length" class="ou-tree">
            <button
              v-for="item in ouTreeItems"
              :key="item.ou.id"
              class="ou-tree-button"
              :class="{ active: selectedOuId === item.ou.id }"
              :style="{ paddingLeft: `${12 + item.depth * 16}px` }"
              @click="selectOu(item.ou)"
            >
              <FolderTree :size="16" />
              <span class="ou-tree-name">{{ item.ou.name }}</span>
              <span class="ou-tree-count">{{ item.userCount }} / {{ item.groupCount }}</span>
            </button>
          </div>
          <div v-else class="sidebar-empty">暂无 OU</div>
        </div>
      </aside>

      <main class="content">
        <section class="view-header">
          <div>
            <h2>{{ currentView.label }}管理</h2>
            <p>维护中心当前事实，Connector 按 revision 主动同步到各域。</p>
          </div>
          <button class="primary-button" :disabled="loading || !tokenReady" @click="refreshAll">
            <RefreshCw :size="17" />
            刷新
          </button>
        </section>

        <p class="status-line" :class="{ error: isError }">{{ statusMessage }}</p>

        <section v-if="activeView === 'directory'" class="directory-workspace">
          <div v-if="selectedOu" class="directory-main">
            <div class="directory-heading">
              <div>
                <h3>{{ selectedOu.name }}</h3>
                <p>{{ selectedOu.id }} / 父级 {{ ouName(selectedOu.parent_id) }}</p>
              </div>
              <div class="row-actions">
                <button class="secondary-button" @click="newOu(selectedOu.id)">新建子 OU</button>
                <button class="secondary-button" @click="selectOu(selectedOu)">编辑 OU</button>
                <button class="primary-button" @click="newUser">
                  <Users :size="17" />
                  新建用户
                </button>
                <button class="primary-button" @click="newGroup">
                  <ShieldCheck :size="17" />
                  新建组
                </button>
              </div>
            </div>

            <div class="entity-panels">
              <div class="panel">
                <div class="panel-header">
                  <h3>用户</h3>
                  <span class="badge">{{ selectedOuUsers.length }}</span>
                </div>
                <div class="table-wrap">
                  <table v-if="selectedOuUsers.length">
                    <thead>
                      <tr>
                        <th>工号</th>
                        <th>姓名</th>
                        <th>联系方式</th>
                        <th>状态</th>
                        <th></th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="user in selectedOuUsers" :key="user.employee_id">
                        <td>{{ user.employee_id }} / {{ user.username }}</td>
                        <td>{{ user.display_name }}</td>
                        <td>{{ user.email ?? user.mobile ?? user.telephone ?? '-' }}</td>
                        <td>
                          <span class="badge" :class="{ warn: user.status === 'disabled' }">
                            {{ user.status === 'active' ? '启用' : '禁用' }}
                          </span>
                        </td>
                        <td>
                          <button class="secondary-button" @click="selectUser(user)">编辑</button>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                  <div v-else class="empty-state">该 OU 下暂无用户</div>
                </div>
              </div>

              <div class="panel">
                <div class="panel-header">
                  <h3>安全组</h3>
                  <span class="badge">{{ selectedOuGroups.length }}</span>
                </div>
                <div class="table-wrap">
                  <table v-if="selectedOuGroups.length">
                    <thead>
                      <tr>
                        <th>组</th>
                        <th>成员数</th>
                        <th>Revision</th>
                        <th></th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="group in selectedOuGroups" :key="group.id">
                        <td>{{ group.name }} / {{ group.id }}</td>
                        <td>{{ group.member_employee_ids.length }}</td>
                        <td>{{ group.changed_revision }}</td>
                        <td>
                          <button class="secondary-button" @click="selectGroup(group)">编辑</button>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                  <div v-else class="empty-state">该 OU 下暂无安全组</div>
                </div>
              </div>
            </div>
          </div>

          <div v-else class="panel empty-state">
            <FolderTree :size="24" />
            <p>先创建 OU，再维护用户和安全组。</p>
            <button class="primary-button" @click="newOu(null)">新建 OU</button>
          </div>

          <aside class="editor-panel">
            <div class="form-switch">
              <button
                class="secondary-button"
                :class="{ active: activeDirectoryForm === 'ou' }"
                @click="activeDirectoryForm = 'ou'"
              >
                OU
              </button>
              <button
                class="secondary-button"
                :class="{ active: activeDirectoryForm === 'user' }"
                @click="newUser"
              >
                用户
              </button>
              <button
                class="secondary-button"
                :class="{ active: activeDirectoryForm === 'group' }"
                @click="newGroup"
              >
                组
              </button>
            </div>

            <form v-if="activeDirectoryForm === 'ou'" class="panel form" @submit.prevent="saveOu">
              <div class="panel-header">
                <h3>{{ editingOuId ? '编辑 OU' : '创建 OU' }}</h3>
              </div>
              <div class="field">
                <label>OU 标识</label>
                <input v-model="ouForm.id" :disabled="!!editingOuId" required />
              </div>
              <div class="field">
                <label>名称</label>
                <input v-model="ouForm.name" required />
              </div>
              <div class="field">
                <label>父 OU</label>
                <select v-model="ouForm.parent_id">
                  <option value="">根 OU</option>
                  <option
                    v-for="item in ouTreeItems"
                    :key="item.ou.id"
                    :disabled="editingOuId === item.ou.id"
                    :value="item.ou.id"
                  >
                    {{ `${'　'.repeat(item.depth)}${item.ou.name}` }}
                  </option>
                </select>
              </div>
              <div class="form-actions">
                <button class="primary-button" :disabled="loading || !tokenReady">
                  <Save :size="17" />
                  保存
                </button>
                <button type="button" class="secondary-button" @click="newOu(selectedOuId)">
                  清空
                </button>
              </div>
            </form>

            <form v-else-if="activeDirectoryForm === 'user'" class="panel form" @submit.prevent="saveUser">
              <div class="panel-header">
                <h3>{{ selectedUserId ? '编辑用户' : '创建用户' }}</h3>
              </div>
              <div class="form-row">
                <div class="field">
                  <label>工号</label>
                  <input v-model="userForm.employee_id" :disabled="!!selectedUserId" required />
                </div>
                <div class="field">
                  <label>登录名</label>
                  <input v-model="userForm.username" required />
                </div>
              </div>
              <div class="field">
                <label>显示名</label>
                <input v-model="userForm.display_name" required />
              </div>
              <div class="field">
                <label>所属 OU</label>
                <select v-model="userForm.organizational_unit_id" required>
                  <option v-for="item in ouTreeItems" :key="item.ou.id" :value="item.ou.id">
                    {{ `${'　'.repeat(item.depth)}${item.ou.name}` }}
                  </option>
                </select>
              </div>
              <div class="form-row">
                <div class="field">
                  <label>邮箱</label>
                  <input v-model="userForm.email" type="email" />
                </div>
                <div class="field">
                  <label>手机</label>
                  <input v-model="userForm.mobile" />
                </div>
              </div>
              <div class="form-row">
                <div class="field">
                  <label>电话</label>
                  <input v-model="userForm.telephone" />
                </div>
                <div class="field">
                  <label>状态</label>
                  <select v-model="userForm.status">
                    <option value="active">启用</option>
                    <option value="disabled">禁用</option>
                  </select>
                </div>
              </div>
              <div v-if="!selectedUserId" class="field">
                <label>初始密码</label>
                <input v-model="userForm.initial_password" type="password" required />
              </div>
              <div v-else class="field">
                <label>重置密码</label>
                <input v-model="userForm.reset_password" type="password" />
              </div>
              <div class="form-actions">
                <button class="primary-button" :disabled="loading || !tokenReady">
                  <Save :size="17" />
                  保存
                </button>
                <button
                  v-if="selectedUserId"
                  type="button"
                  class="secondary-button"
                  :disabled="loading || !tokenReady"
                  @click="setUserEnabled(true)"
                >
                  启用
                </button>
                <button
                  v-if="selectedUserId"
                  type="button"
                  class="danger-button"
                  :disabled="loading || !tokenReady"
                  @click="setUserEnabled(false)"
                >
                  禁用
                </button>
                <button
                  v-if="selectedUserId"
                  type="button"
                  class="secondary-button"
                  :disabled="loading || !tokenReady || !userForm.reset_password"
                  @click="resetUserPassword"
                >
                  重置密码
                </button>
                <button type="button" class="secondary-button" @click="newUser">清空</button>
              </div>
            </form>

            <form v-else class="panel form" @submit.prevent="saveGroup">
              <div class="panel-header">
                <h3>{{ selectedGroupId ? '编辑组' : '创建组' }}</h3>
              </div>
              <div class="field">
                <label>组标识</label>
                <input v-model="groupForm.id" :disabled="!!selectedGroupId" required />
              </div>
              <div class="field">
                <label>组名称</label>
                <input v-model="groupForm.name" required />
              </div>
              <div class="field">
                <label>所属 OU</label>
                <select v-model="groupForm.organizational_unit_id" required>
                  <option v-for="item in ouTreeItems" :key="item.ou.id" :value="item.ou.id">
                    {{ `${'　'.repeat(item.depth)}${item.ou.name}` }}
                  </option>
                </select>
              </div>
              <div class="field">
                <label>成员工号</label>
                <textarea v-model="groupForm.member_employee_ids" placeholder="用英文逗号分隔"></textarea>
              </div>
              <div class="form-actions">
                <button class="primary-button" :disabled="loading || !tokenReady">
                  <Save :size="17" />
                  保存
                </button>
                <button type="button" class="secondary-button" @click="newGroup">清空</button>
              </div>
            </form>
          </aside>
        </section>

        <section v-else-if="activeView === 'domains'" class="workspace-grid">
          <div class="panel">
            <div class="panel-header">
              <h3>域列表</h3>
              <button class="secondary-button" @click="newDomain">新建</button>
            </div>
            <div class="table-wrap">
              <table v-if="domains.length">
                <thead>
                  <tr>
                    <th>域</th>
                    <th>UPN</th>
                    <th>状态</th>
                    <th>目录</th>
                    <th>凭据</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="domain in domains" :key="domain.id">
                    <td>{{ domain.name }} / {{ domain.id }}</td>
                    <td>{{ domain.upn_suffix }}</td>
                    <td>
                      <span class="badge" :class="{ warn: !domain.enabled }">
                        {{ domain.enabled ? '启用' : '停用' }}
                      </span>
                    </td>
                    <td>{{ domain.applied_directory_revision }}</td>
                    <td>{{ domain.applied_credential_revision }}</td>
                    <td><button class="secondary-button" @click="selectDomain(domain)">编辑</button></td>
                  </tr>
                </tbody>
              </table>
              <div v-else class="empty-state">暂无域配置</div>
            </div>
          </div>

          <form class="panel form" @submit.prevent="saveDomain">
            <div class="panel-header">
              <h3>{{ selectedDomainId ? '编辑域' : '创建域' }}</h3>
            </div>
            <div class="form-row">
              <div class="field">
                <label>域标识</label>
                <input v-model="domainForm.id" :disabled="!!selectedDomainId" required />
              </div>
              <div class="field">
                <label>域名称</label>
                <input v-model="domainForm.name" required />
              </div>
            </div>
            <div class="field">
              <label>UPN 后缀</label>
              <input v-model="domainForm.upn_suffix" required />
            </div>
            <div class="field">
              <label>镜像根 DN</label>
              <input v-model="domainForm.mirror_root_dn" required />
            </div>
            <div class="field">
              <label>隔离 OU DN</label>
              <input v-model="domainForm.quarantine_ou_dn" required />
            </div>
            <div class="form-row">
              <div class="field">
                <label>工号属性</label>
                <input v-model="domainForm.employee_id_attribute" required />
              </div>
              <div class="field">
                <label>受管组标识属性</label>
                <input v-model="domainForm.managed_group_id_attribute" required />
              </div>
            </div>
            <div v-if="!selectedDomainId" class="field">
              <label>Connector key</label>
              <input v-model="domainForm.connector_key" required />
            </div>
            <div class="field">
              <label>状态</label>
              <select v-model="domainForm.enabled">
                <option :value="true">启用</option>
                <option :value="false">停用</option>
              </select>
            </div>
            <div class="form-actions">
              <button class="primary-button" :disabled="loading || !tokenReady">
                <Save :size="17" />
                保存
              </button>
              <button type="button" class="secondary-button" @click="newDomain">清空</button>
            </div>
          </form>
        </section>

        <section v-else class="panel">
          <div class="panel-header">
            <h3>域同步状态</h3>
            <div class="row-actions">
              <span class="badge">目录滞后 {{ activeDirectoryLag }}</span>
              <span class="badge" :class="{ warn: activeCredentialLag > 0 }">
                凭据滞后 {{ activeCredentialLag }}
              </span>
            </div>
          </div>
          <div class="table-wrap">
            <table v-if="syncDomains.length">
              <thead>
                <tr>
                  <th>域</th>
                  <th>状态</th>
                  <th>已确认目录</th>
                  <th>已确认凭据</th>
                  <th>目录滞后</th>
                  <th>凭据滞后</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="domain in syncDomains" :key="domain.domain_id">
                  <td>{{ domain.domain_id }}</td>
                  <td>
                    <span class="badge" :class="{ warn: !domain.enabled }">
                      {{ domain.enabled ? '启用' : '停用' }}
                    </span>
                  </td>
                  <td>{{ domain.applied_directory_revision }}</td>
                  <td>{{ domain.applied_credential_revision }}</td>
                  <td>{{ domain.directory_lag }}</td>
                  <td>{{ domain.credential_lag }}</td>
                </tr>
              </tbody>
            </table>
            <div v-else class="empty-state">暂无同步状态</div>
          </div>
        </section>

        <section v-if="!tokenReady" class="empty-state">
          <CheckCircle2 :size="22" />
          <p>输入管理凭证后即可读取和编辑中心事实。</p>
        </section>
      </main>
    </div>
  </div>
</template>
