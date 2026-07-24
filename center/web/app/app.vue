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

type ViewKey = 'domains' | 'ous' | 'users' | 'groups' | 'sync'
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
  member_employee_ids: string
}

const views: Array<{ key: ViewKey; label: string; icon: typeof Building2 }> = [
  { key: 'domains', label: '域', icon: Building2 },
  { key: 'ous', label: 'OU', icon: FolderTree },
  { key: 'users', label: '用户', icon: Users },
  { key: 'groups', label: '组', icon: ShieldCheck },
  { key: 'sync', label: '同步', icon: Workflow },
]
const defaultView = views[0]!

const activeView = ref<ViewKey>('domains')
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
const selectedUserId = ref<string | null>(null)
const selectedGroupId = ref<string | null>(null)

const domainForm = reactive<DomainForm>(blankDomainForm())
const ouForm = reactive<OuForm>(blankOuForm())
const userForm = reactive<UserForm>(blankUserForm())
const groupForm = reactive<GroupForm>(blankGroupForm())

const currentView = computed(() => views.find((view) => view.key === activeView.value) ?? defaultView)
const tokenReady = computed(() => managementToken.value.trim().length > 0)
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

function selectOu(ou: OrganizationalUnit) {
  selectedOuId.value = ou.id
  assignForm(ouForm, {
    id: ou.id,
    name: ou.name,
    parent_id: ou.parent_id ?? '',
  })
}

function newOu() {
  selectedOuId.value = null
  assignForm(ouForm, blankOuForm())
}

async function saveOu() {
  await runAction(async () => {
    const payload = {
      name: ouForm.name,
      parent_id: optionalParentId(ouForm.parent_id),
    }
    if (selectedOuId.value) {
      await adminFetch(`/api/admin/ous/${encodeURIComponent(selectedOuId.value)}`, {
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
    }
    await loadOus()
    await loadSyncDomains()
  }, selectedOuId.value ? 'OU 已更新' : 'OU 已创建')
}

function selectUser(user: UserRecord) {
  selectedUserId.value = user.employee_id
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
  selectedUserId.value = null
  assignForm(userForm, blankUserForm())
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
  selectedGroupId.value = group.id
  assignForm(groupForm, {
    id: group.id,
    name: group.name,
    member_employee_ids: group.member_employee_ids.join(', '),
  })
}

function newGroup() {
  selectedGroupId.value = null
  assignForm(groupForm, blankGroupForm())
}

async function saveGroup() {
  await runAction(async () => {
    if (selectedGroupId.value) {
      await adminFetch(`/api/admin/groups/${encodeURIComponent(selectedGroupId.value)}`, {
        method: 'PATCH',
        body: JSON.stringify({ name: groupForm.name }),
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
          name: groupForm.name,
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
        <nav class="nav-list">
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

        <section v-if="activeView === 'domains'" class="workspace-grid">
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

        <section v-else-if="activeView === 'ous'" class="workspace-grid">
          <div class="panel">
            <div class="panel-header">
              <h3>OU 树</h3>
              <button class="secondary-button" @click="newOu">新建</button>
            </div>
            <div class="table-wrap">
              <table v-if="organizationalUnits.length">
                <thead>
                  <tr>
                    <th>OU</th>
                    <th>父级</th>
                    <th>Revision</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="ou in organizationalUnits" :key="ou.id">
                    <td>{{ ou.name }} / {{ ou.id }}</td>
                    <td>{{ ou.parent_id ?? '-' }}</td>
                    <td>{{ ou.changed_revision }}</td>
                    <td><button class="secondary-button" @click="selectOu(ou)">编辑</button></td>
                  </tr>
                </tbody>
              </table>
              <div v-else class="empty-state">暂无 OU</div>
            </div>
          </div>

          <form class="panel form" @submit.prevent="saveOu">
            <div class="panel-header">
              <h3>{{ selectedOuId ? '编辑 OU' : '创建 OU' }}</h3>
            </div>
            <div class="field">
              <label>OU 标识</label>
              <input v-model="ouForm.id" :disabled="!!selectedOuId" required />
            </div>
            <div class="field">
              <label>名称</label>
              <input v-model="ouForm.name" required />
            </div>
            <div class="field">
              <label>父 OU 标识</label>
              <input v-model="ouForm.parent_id" placeholder="根 OU 留空" />
            </div>
            <div class="form-actions">
              <button class="primary-button" :disabled="loading || !tokenReady">
                <Save :size="17" />
                保存
              </button>
              <button type="button" class="secondary-button" @click="newOu">清空</button>
            </div>
          </form>
        </section>

        <section v-else-if="activeView === 'users'" class="workspace-grid">
          <div class="panel">
            <div class="panel-header">
              <h3>用户列表</h3>
              <button class="secondary-button" @click="newUser">新建</button>
            </div>
            <div class="table-wrap">
              <table v-if="users.length">
                <thead>
                  <tr>
                    <th>工号</th>
                    <th>姓名</th>
                    <th>OU</th>
                    <th>状态</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="user in users" :key="user.employee_id">
                    <td>{{ user.employee_id }} / {{ user.username }}</td>
                    <td>{{ user.display_name }}</td>
                    <td>{{ user.organizational_unit_id }}</td>
                    <td>
                      <span class="badge" :class="{ warn: user.status === 'disabled' }">
                        {{ user.status === 'active' ? '启用' : '禁用' }}
                      </span>
                    </td>
                    <td><button class="secondary-button" @click="selectUser(user)">编辑</button></td>
                  </tr>
                </tbody>
              </table>
              <div v-else class="empty-state">暂无用户</div>
            </div>
          </div>

          <form class="panel form" @submit.prevent="saveUser">
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
                <label>OU 标识</label>
                <input v-model="userForm.organizational_unit_id" required />
              </div>
            </div>
            <div class="field">
              <label>状态</label>
              <select v-model="userForm.status">
                <option value="active">启用</option>
                <option value="disabled">禁用</option>
              </select>
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
        </section>

        <section v-else-if="activeView === 'groups'" class="workspace-grid">
          <div class="panel">
            <div class="panel-header">
              <h3>安全组</h3>
              <button class="secondary-button" @click="newGroup">新建</button>
            </div>
            <div class="table-wrap">
              <table v-if="groups.length">
                <thead>
                  <tr>
                    <th>组</th>
                    <th>成员数</th>
                    <th>Revision</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="group in groups" :key="group.id">
                    <td>{{ group.name }} / {{ group.id }}</td>
                    <td>{{ group.member_employee_ids.length }}</td>
                    <td>{{ group.changed_revision }}</td>
                    <td><button class="secondary-button" @click="selectGroup(group)">编辑</button></td>
                  </tr>
                </tbody>
              </table>
              <div v-else class="empty-state">暂无安全组</div>
            </div>
          </div>

          <form class="panel form" @submit.prevent="saveGroup">
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
