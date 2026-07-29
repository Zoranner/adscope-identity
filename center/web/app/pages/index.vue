<script setup lang="ts">
import { FolderTree, ShieldCheck, Users } from 'lucide-vue-next'
import GroupEditor from '~/components/directory/GroupEditor.vue'
import ObjectTable from '~/components/directory/ObjectTable.vue'
import OuEditor from '~/components/directory/OuEditor.vue'
import OuTree from '~/components/directory/OuTree.vue'
import UserEditor from '~/components/directory/UserEditor.vue'
import type { GroupForm, GroupRecord, OuForm, UserForm, UserRecord } from '~/types/admin'
import { flattenOus, ouName, sortOus } from '~/utils/directory'
import {
  blankGroupForm,
  blankOuForm,
  blankUserForm,
  nullable,
  splitMembers,
} from '~/utils/forms'

type EditorKey = 'ou' | 'user' | 'group'

const {
  tokenReady,
  loading,
  organizationalUnits,
  users,
  groups,
  adminFetch,
  runAction,
  loadOus,
  loadUsers,
  loadGroups,
  loadSyncDomains,
  refreshDirectory,
} = useAdminApi()
const { setStatus } = useAdminStatus()

const selectedOuId = ref<string | null>(null)
const activeModal = ref<EditorKey | null>(null)
const editingOuId = ref<string | null>(null)
const selectedUserId = ref<string | null>(null)
const selectedGroupId = ref<string | null>(null)
const ouForm = reactive<OuForm>(blankOuForm())
const userForm = reactive<UserForm>(blankUserForm())
const groupForm = reactive<GroupForm>(blankGroupForm())

const treeItems = computed(() =>
  flattenOus(organizationalUnits.value, users.value, groups.value),
)
const selectedOu = computed(
  () => organizationalUnits.value.find((ou) => ou.id === selectedOuId.value) ?? null,
)
const selectedUsers = computed(() =>
  selectedOuId.value
    ? users.value.filter((user) => user.organizational_unit_id === selectedOuId.value)
    : [],
)
const selectedGroups = computed(() =>
  selectedOuId.value
    ? groups.value.filter((group) => group.organizational_unit_id === selectedOuId.value)
    : [],
)
const modalTitle = computed(() => {
  if (activeModal.value === 'ou') {
    return editingOuId.value ? '编辑 OU' : '创建 OU'
  }
  if (activeModal.value === 'user') {
    return selectedUserId.value ? '编辑用户' : '创建用户'
  }
  if (activeModal.value === 'group') {
    return selectedGroupId.value ? '编辑安全组' : '创建安全组'
  }
  return ''
})

watch(
  organizationalUnits,
  () => {
    if (organizationalUnits.value.length === 0) {
      selectedOuId.value = null
      return
    }
    const current = organizationalUnits.value.find((ou) => ou.id === selectedOuId.value)
    if (!current) {
      selectedOuId.value = sortOus(organizationalUnits.value)[0]!.id
    }
  },
  { immediate: true },
)

function assignForm<T extends object>(target: T, source: T) {
  Object.assign(target, source)
}

function selectOu(id: string) {
  const ou = organizationalUnits.value.find((item) => item.id === id)
  if (!ou) {
    return
  }
  selectedOuId.value = id
}

function editOu(id: string) {
  const ou = organizationalUnits.value.find((item) => item.id === id)
  if (!ou) {
    return
  }
  selectedOuId.value = id
  activeModal.value = 'ou'
  editingOuId.value = id
  assignForm(ouForm, {
    id: ou.id,
    name: ou.name,
    parent_id: ou.parent_id ?? '',
  })
}

function newOu(parentId: string | null = selectedOuId.value) {
  activeModal.value = 'ou'
  editingOuId.value = null
  selectedUserId.value = null
  selectedGroupId.value = null
  assignForm(ouForm, {
    ...blankOuForm(),
    parent_id: parentId ?? '',
  })
}

async function saveOu() {
  await runAction(async () => {
    const payload = {
      name: ouForm.name,
      parent_id: nullable(ouForm.parent_id),
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
    await Promise.all([loadOus(), loadSyncDomains()])
    closeModal()
  }, { successMessage: editingOuId.value ? 'OU 已更新' : 'OU 已创建' })
}

function newUser() {
  if (!selectedOuId.value) {
    setStatus('请先选择 OU', true)
    return
  }
  activeModal.value = 'user'
  selectedUserId.value = null
  selectedGroupId.value = null
  assignForm(userForm, {
    ...blankUserForm(),
    organizational_unit_id: selectedOuId.value,
  })
}

function editUser(user: UserRecord) {
  selectedOuId.value = user.organizational_unit_id
  activeModal.value = 'user'
  selectedUserId.value = user.employee_id
  selectedGroupId.value = null
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
    selectedOuId.value = userForm.organizational_unit_id
    await Promise.all([loadUsers(), loadSyncDomains()])
    closeModal()
  }, { successMessage: selectedUserId.value ? '用户已更新' : '用户已创建' })
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
    await Promise.all([loadUsers(), loadSyncDomains()])
  }, { successMessage: enabled ? '用户已启用' : '用户已禁用' })
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
  }, { successMessage: '用户密码已重置' })
}

function newGroup() {
  if (!selectedOuId.value) {
    setStatus('请先选择 OU', true)
    return
  }
  activeModal.value = 'group'
  selectedGroupId.value = null
  selectedUserId.value = null
  assignForm(groupForm, {
    ...blankGroupForm(),
    organizational_unit_id: selectedOuId.value,
  })
}

function editGroup(group: GroupRecord) {
  selectedOuId.value = group.organizational_unit_id
  activeModal.value = 'group'
  selectedGroupId.value = group.id
  selectedUserId.value = null
  assignForm(groupForm, {
    id: group.id,
    name: group.name,
    organizational_unit_id: group.organizational_unit_id,
    member_employee_ids: group.member_employee_ids.join(', '),
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
    selectedOuId.value = groupForm.organizational_unit_id
    await Promise.all([loadGroups(), loadSyncDomains()])
    closeModal()
  }, { successMessage: selectedGroupId.value ? '组已更新' : '组已创建' })
}

function closeModal() {
  activeModal.value = null
}
</script>

<template>
  <AdminShell>
    <AdminPageHeader
      title="目录管理"
      description="按 OU 维护中心当前事实，用户和安全组都从所选 OU 进入。"
      :loading="loading"
      :disabled="!tokenReady"
      @refresh="refreshDirectory"
    />

    <section class="directory-page">
      <OuTree
        :items="treeItems"
        :selected-id="selectedOuId"
        :disabled="!tokenReady"
        @select="selectOu"
        @create="newOu"
      />

      <div class="directory-content">
        <template v-if="selectedOu">
          <section class="selection-summary">
            <div>
              <h2>{{ selectedOu.name }}</h2>
              <p>{{ selectedOu.id }} / 父级 {{ ouName(organizationalUnits, selectedOu.parent_id) }}</p>
            </div>
            <div class="row-actions">
              <button class="secondary-button" @click="newOu(selectedOu.id)">
                <FolderTree :size="16" />
                子 OU
              </button>
              <button class="secondary-button" @click="editOu(selectedOu.id)">编辑 OU</button>
              <button class="primary-button" @click="newUser">
                <Users :size="16" />
                用户
              </button>
              <button class="primary-button" @click="newGroup">
                <ShieldCheck :size="16" />
                安全组
              </button>
            </div>
          </section>

          <ObjectTable
            :users="selectedUsers"
            :groups="selectedGroups"
            :disabled="!selectedOuId"
            @create-user="newUser"
            @create-group="newGroup"
            @edit-user="editUser"
            @edit-group="editGroup"
          />
        </template>

        <section v-else class="panel directory-empty-panel">
          <AdminEmptyState
            :title="tokenReady ? '暂无组织单元' : '未连接管理入口'"
            :action-label="tokenReady ? '新建 OU' : undefined"
            @action="newOu(null)"
          />
        </section>
      </div>
    </section>

    <AdminModal :open="activeModal !== null" :title="modalTitle" width="wide" @close="closeModal">
      <OuEditor
        v-if="activeModal === 'ou'"
        v-model="ouForm"
        :items="treeItems"
        :editing-id="editingOuId"
        :loading="loading"
        :disabled="!tokenReady"
        @save="saveOu"
        @reset="newOu(selectedOuId)"
      />
      <UserEditor
        v-else-if="activeModal === 'user'"
        v-model="userForm"
        :items="treeItems"
        :editing-id="selectedUserId"
        :loading="loading"
        :disabled="!tokenReady"
        @save="saveUser"
        @reset="newUser"
        @enable="setUserEnabled(true)"
        @disable="setUserEnabled(false)"
        @reset-password="resetUserPassword"
      />
      <GroupEditor
        v-else-if="activeModal === 'group'"
        v-model="groupForm"
        :items="treeItems"
        :editing-id="selectedGroupId"
        :loading="loading"
        :disabled="!tokenReady"
        @save="saveGroup"
        @reset="newGroup"
      />
    </AdminModal>
  </AdminShell>
</template>
