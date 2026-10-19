<script setup lang="ts">
import {
  Button,
  Drawer,
  EmptyState,
  Icon,
  Input,
  Modal,
  RadioGroup,
  Tree,
  useNotification,
} from '@stellar/ui'
import GroupEditor from '~/components/directory/GroupEditor.vue'
import GroupTable from '~/components/directory/GroupTable.vue'
import OuEditor from '~/components/directory/OuEditor.vue'
import UserEditor from '~/components/directory/UserEditor.vue'
import UserTable from '~/components/directory/UserTable.vue'
import EditorForm from '~/components/shell/EditorForm.vue'
import type {
  GroupForm,
  GroupRecord,
  OrganizationalUnit,
  OuForm,
  UserForm,
  UserRecord,
} from '~/types/admin'
import { buildOuTree, flattenOus, sortOus } from '~/utils/directory'
import {
  blankGroupForm,
  blankOuForm,
  blankUserForm,
  nullable,
  splitMembers,
} from '~/utils/forms'

type EditorKey = 'ou' | 'user' | 'group'

const {
  pending,
  organizationalUnits,
  users,
  groups,
  directoryLoading,
  adminFetch,
  runAction,
  loadOus,
  loadUsers,
  loadGroups,
  loadSyncDomains,
  refreshDirectory,
} = useAdminApi()
const { confirm } = useConfirm()
const notifications = useNotification()

const selectedOuId = ref<string | null>(null)
const activeTab = ref('users')

const sectionOptions = [
  { value: 'users', label: '用户', icon: 'ph:users' },
  { value: 'groups', label: '安全组', icon: 'ph:shield' },
]
const editor = ref<EditorKey | null>(null)
const saving = ref(false)
const editingOuId = ref<string | null>(null)
const editingUserId = ref<string | null>(null)
const editingGroupId = ref<string | null>(null)
const ouForm = reactive<OuForm>(blankOuForm())
const userForm = reactive<UserForm>(blankUserForm())
const groupForm = reactive<GroupForm>(blankGroupForm())
const resetPasswordTarget = ref<UserRecord | null>(null)
const resetPasswordValue = ref('')

const busy = computed(() => saving.value || pending.value > 0)
const treeItems = computed(() => flattenOus(organizationalUnits.value))
const treeData = computed(() => buildOuTree(organizationalUnits.value))
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
const selectedKeys = computed<(string | number)[]>({
  get: () => (selectedOuId.value ? [selectedOuId.value] : []),
  set: (keys) => {
    const last = keys[keys.length - 1]
    selectedOuId.value = last === undefined ? null : String(last)
  },
})

const ouEditorOpen = computed({
  get: () => editor.value === 'ou',
  set: (value: boolean) => {
    if (!value && !saving.value) {
      editor.value = null
    }
  },
})
const userEditorOpen = computed({
  get: () => editor.value === 'user',
  set: (value: boolean) => {
    if (!value && !saving.value) {
      editor.value = null
    }
  },
})
const groupEditorOpen = computed({
  get: () => editor.value === 'group',
  set: (value: boolean) => {
    if (!value && !saving.value) {
      editor.value = null
    }
  },
})
const resetPasswordOpen = computed({
  get: () => resetPasswordTarget.value !== null,
  set: (value: boolean) => {
    if (!value && !saving.value) {
      resetPasswordTarget.value = null
      resetPasswordValue.value = ''
    }
  },
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

function openOuEditor(ou?: OrganizationalUnit, parentId: string | null = selectedOuId.value) {
  editingOuId.value = ou?.id ?? null
  editingUserId.value = null
  editingGroupId.value = null
  assignForm(ouForm, {
    ...blankOuForm(),
    id: ou?.id ?? '',
    name: ou?.name ?? '',
    parent_id: ou?.parent_id ?? parentId ?? '',
  })
  editor.value = 'ou'
}

function ouForNode(node: { key?: string | number }): OrganizationalUnit | undefined {
  return organizationalUnits.value.find((ou) => ou.id === String(node.key))
}

function editOuNode(node: { key?: string | number }) {
  const ou = ouForNode(node)
  if (ou) {
    openOuEditor(ou)
  }
}

function removeOuNode(node: { key?: string | number }) {
  const ou = ouForNode(node)
  if (ou) {
    void removeOu(ou)
  }
}

async function openUserEditor(user?: UserRecord) {
  if (!user && !selectedOuId.value) {
    return
  }
  let employeeId = user?.employee_id ?? ''
  if (!user) {
    let nextEmployeeId = ''
    await runAction(async () => {
      const response = await adminFetch<{ employee_id: string }>(
        '/api/admin/users/next-employee-id',
      )
      nextEmployeeId = response.employee_id
    })
    if (!nextEmployeeId) {
      return
    }
    employeeId = nextEmployeeId
  }
  editingUserId.value = user?.employee_id ?? null
  editingOuId.value = null
  editingGroupId.value = null
  assignForm(userForm, {
    ...blankUserForm(),
    employee_id: employeeId,
    username: user?.username ?? '',
    display_name: user?.display_name ?? '',
    surname: user?.surname ?? '',
    given_name: user?.given_name ?? '',
    email: user?.email ?? '',
    mobile: user?.mobile ?? '',
    organizational_unit_id: user?.organizational_unit_id ?? selectedOuId.value ?? '',
    status: user?.status ?? 'active',
  })
  editor.value = 'user'
}

function openGroupEditor(group?: GroupRecord) {
  if (!group && !selectedOuId.value) {
    return
  }
  editingGroupId.value = group?.id ?? null
  editingOuId.value = null
  editingUserId.value = null
  assignForm(groupForm, {
    ...blankGroupForm(),
    id: group?.id ?? '',
    name: group?.name ?? '',
    organizational_unit_id: group?.organizational_unit_id ?? selectedOuId.value ?? '',
    member_employee_ids: group?.member_employee_ids.join(', ') ?? '',
  })
  editor.value = 'group'
}

function openResetPassword(user: UserRecord) {
  resetPasswordTarget.value = user
  resetPasswordValue.value = ''
}

async function saveOu() {
  saving.value = true
  try {
    await runAction(
      async () => {
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
            body: JSON.stringify({ id: ouForm.id, ...payload }),
          })
          selectedOuId.value = ouForm.id
        }
        await Promise.all([loadOus(), loadSyncDomains()])
        editor.value = null
      },
      { successMessage: editingOuId.value ? 'OU 已更新' : 'OU 已创建' },
    )
  } finally {
    saving.value = false
  }
}

async function removeOu(ou: OrganizationalUnit) {
  const hasChildren = organizationalUnits.value.some((item) => item.parent_id === ou.id)
  const hasUsers = users.value.some((user) => user.organizational_unit_id === ou.id)
  const hasGroups = groups.value.some((group) => group.organizational_unit_id === ou.id)
  if (hasChildren || hasUsers || hasGroups) {
    notifications.warning('该 OU 还有子 OU、用户或安全组，不能删除')
    return
  }

  const confirmed = await confirm({
    title: '删除组织单元',
    message: `删除 ${ou.name}？`,
    description: '仅空 OU 可以删除；AD 中的 OU 对象不会物理删除。',
    confirmText: '删除',
    danger: true,
  })
  if (!confirmed) {
    return
  }

  saving.value = true
  try {
    await runAction(
      async () => {
        await adminFetch(`/api/admin/ous/${encodeURIComponent(ou.id)}`, {
          method: 'DELETE',
        })
        if (selectedOuId.value === ou.id) {
          selectedOuId.value = null
        }
        if (editingOuId.value === ou.id) {
          editor.value = null
          editingOuId.value = null
        }
        await Promise.all([loadOus(), loadUsers(), loadGroups(), loadSyncDomains()])
      },
      { successMessage: '组织单元已删除' },
    )
  } finally {
    saving.value = false
  }
}

async function saveUser() {
  saving.value = true
  try {
    await runAction(
      async () => {
        const payload = {
          username: userForm.username,
          display_name: userForm.display_name,
          surname: userForm.surname,
          given_name: userForm.given_name,
          email: nullable(userForm.email),
          mobile: nullable(userForm.mobile),
          organizational_unit_id: userForm.organizational_unit_id,
          status: userForm.status,
        }
        if (editingUserId.value) {
          await adminFetch(`/api/admin/users/${encodeURIComponent(editingUserId.value)}`, {
            method: 'PATCH',
            body: JSON.stringify(payload),
          })
        } else {
          await adminFetch('/api/admin/users', {
            method: 'POST',
            body: JSON.stringify({
              initial_password: userForm.initial_password,
              ...payload,
            }),
          })
        }
        selectedOuId.value = userForm.organizational_unit_id
        await Promise.all([loadUsers(), loadSyncDomains()])
        editor.value = null
      },
      { successMessage: editingUserId.value ? '用户已更新' : '用户已创建' },
    )
  } finally {
    saving.value = false
  }
}

async function saveGroup() {
  saving.value = true
  try {
    await runAction(
      async () => {
        const payload = {
          name: groupForm.name,
          organizational_unit_id: groupForm.organizational_unit_id,
        }
        const members = splitMembers(groupForm.member_employee_ids)
        if (editingGroupId.value) {
          await adminFetch(`/api/admin/groups/${encodeURIComponent(editingGroupId.value)}`, {
            method: 'PATCH',
            body: JSON.stringify(payload),
          })
          await adminFetch(`/api/admin/groups/${encodeURIComponent(editingGroupId.value)}/members`, {
            method: 'PUT',
            body: JSON.stringify({ member_employee_ids: members }),
          })
        } else {
          await adminFetch('/api/admin/groups', {
            method: 'POST',
            body: JSON.stringify({ id: groupForm.id, ...payload }),
          })
          if (members.length > 0) {
            await adminFetch(`/api/admin/groups/${encodeURIComponent(groupForm.id)}/members`, {
              method: 'PUT',
              body: JSON.stringify({ member_employee_ids: members }),
            })
          }
        }
        selectedOuId.value = groupForm.organizational_unit_id
        await Promise.all([loadGroups(), loadSyncDomains()])
        editor.value = null
      },
      { successMessage: editingGroupId.value ? '组已更新' : '组已创建' },
    )
  } finally {
    saving.value = false
  }
}

async function removeUser(user: UserRecord) {
  const confirmed = await confirm({
    title: '删除账号',
    message: `删除 ${user.display_name}（${user.employee_id}）？`,
    description: '中心记录会删除，所属域会禁用该账号并移到隔离 OU。',
    confirmText: '删除',
    danger: true,
  })
  if (!confirmed) {
    return
  }

  saving.value = true
  try {
    await runAction(
      async () => {
        await adminFetch(`/api/admin/users/${encodeURIComponent(user.employee_id)}`, {
          method: 'DELETE',
        })
        if (editingUserId.value === user.employee_id) {
          editor.value = null
          editingUserId.value = null
        }
        if (resetPasswordTarget.value?.employee_id === user.employee_id) {
          resetPasswordTarget.value = null
          resetPasswordValue.value = ''
        }
        await Promise.all([loadUsers(), loadGroups(), loadSyncDomains()])
      },
      { successMessage: '账号已删除，域将禁用并隔离该账号' },
    )
  } finally {
    saving.value = false
  }
}

async function resetUserPassword() {
  const target = resetPasswordTarget.value
  if (!target) {
    return
  }
  saving.value = true
  try {
    await runAction(
      async () => {
        await adminFetch(
          `/api/admin/users/${encodeURIComponent(target.employee_id)}/password-reset`,
          {
            method: 'POST',
            body: JSON.stringify({ new_password: resetPasswordValue.value }),
          },
        )
        await loadSyncDomains()
        resetPasswordTarget.value = null
        resetPasswordValue.value = ''
      },
      { successMessage: '密码已重置，正在同步到所属域' },
    )
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <AdminShell title="目录">
    <template #actions>
      <Button icon="ph:arrows-clockwise" @click="refreshDirectory">
        刷新
      </Button>
    </template>

    <div
      class="grid min-h-0 flex-1 items-stretch gap-[var(--spacing-lg)] grid-cols-[minmax(15rem,17rem)_minmax(0,1fr)] max-[960px]:grid-cols-1"
    >
      <div class="min-h-0 max-[960px]:max-h-[45dvh]">
        <Card title="组织单元" :hoverable="false" full-height padded="header">
        <template #actions>
          <Button
            size="small"
            square
            icon="ph:plus"
            aria-label="新建组织单元"
            title="新建组织单元"
            :disabled="busy"
            @click="openOuEditor()"
          />
        </template>

        <div
          v-if="treeData.length > 0"
          class="flex min-h-0 flex-1 flex-col overflow-auto"
        >
          <Tree
            v-model:selected-keys="selectedKeys"
            :data="treeData"
            selectable
            default-expand-all
          >
            <template #icon="{ expanded }">
              <Icon :icon="expanded ? 'ph:folder-open' : 'ph:folder'" class="h-4 w-4" />
            </template>
            <template #extra="{ node }">
              <span
                class="pointer-events-none flex items-center gap-[var(--spacing-xs)] opacity-0 transition-opacity group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100"
              >
                <Button
                  size="small"
                  square
                  icon="ph:pencil-simple"
                  title="编辑组织单元"
                  aria-label="编辑组织单元"
                  :disabled="busy"
                  @click.stop="editOuNode(node)"
                />
                <Button
                  size="small"
                  square
                  variant="solid"
                  semantic="error"
                  icon="ph:trash"
                  title="删除组织单元"
                  aria-label="删除组织单元"
                  :disabled="busy"
                  @click.stop="removeOuNode(node)"
                />
              </span>
            </template>
          </Tree>
        </div>
        <div
          v-else
          class="grid min-h-0 flex-1 place-items-center gap-[var(--spacing-xs)] overflow-auto"
        >
          <EmptyState
            title="还没有组织单元"
            description="先创建 OU，再维护用户和安全组。"
          />
        </div>
        </Card>
      </div>

      <div v-if="selectedOu" class="flex min-h-0 min-w-0 flex-col gap-[var(--spacing-md)]">
        <div
          class="grid flex-none grid-cols-[minmax(0,1fr)_auto] items-center gap-[var(--spacing-md)]"
        >
          <RadioGroup
            v-model="activeTab"
            :options="sectionOptions"
            variant="button"
            aria-label="用户与安全组"
          />
          <Button
            v-if="activeTab === 'groups'"
            variant="solid"
            semantic="primary"
            icon="ph:shield-plus"
            :disabled="busy"
            @click="openGroupEditor()"
          >
            新建
          </Button>
          <Button
            v-else
            variant="solid"
            semantic="primary"
            icon="ph:user-plus"
            :disabled="busy"
            @click="openUserEditor()"
          >
            新建
          </Button>
        </div>

        <UserTable
          v-if="activeTab === 'users'"
          :users="selectedUsers"
          :loading="directoryLoading"
          :busy="busy"
          @edit="openUserEditor"
          @reset-password="openResetPassword"
          @remove="removeUser"
        />
        <GroupTable
          v-else
          :groups="selectedGroups"
          :loading="directoryLoading"
          :busy="busy"
          @edit="openGroupEditor"
        />
      </div>

      <section
        v-else
        class="grid min-h-0 flex-1 place-items-center rounded-[var(--radius-lg)] border border-[var(--st-border-divider)]"
      >
        <EmptyState
          title="还没有选择组织单元"
          :description="
            organizationalUnits.length > 0
              ? '在左侧 OU 树中选择一个 OU，查看它的用户和安全组。'
              : '先创建第一个 OU。'
          "
        />
      </section>
    </div>

    <Drawer
      v-model:visible="ouEditorOpen"
      :title="editingOuId ? '编辑 OU' : '新建 OU'"
      :blocked="saving"
      :disabled="saving"
    >
      <OuEditor
        v-model="ouForm"
        :items="treeItems"
        :editing-id="editingOuId"
        :saving="saving"
        @save="saveOu"
      />
      <template #footer>
        <Button :disabled="saving" @click="ouEditorOpen = false">取消</Button>
        <Button
          form="ou-form"
          type="submit"
          variant="solid"
          semantic="primary"
          icon="ph:floppy-disk"
          :disabled="saving"
        >
          保存
        </Button>
      </template>
    </Drawer>

    <Drawer
      v-model:visible="userEditorOpen"
      :title="editingUserId ? '编辑用户' : '新建用户'"
      :blocked="saving"
      :disabled="saving"
    >
      <UserEditor
        v-model="userForm"
        :items="treeItems"
        :editing-id="editingUserId"
        @save="saveUser"
      />
      <template #footer>
        <Button :disabled="saving" @click="userEditorOpen = false">取消</Button>
        <Button
          form="user-form"
          type="submit"
          variant="solid"
          semantic="primary"
          icon="ph:floppy-disk"
          :disabled="saving"
        >
          保存
        </Button>
      </template>
    </Drawer>

    <Modal
      v-model:visible="resetPasswordOpen"
      title="重置密码"
      :blocked="saving"
      :close-on-overlay="!saving"
      :close-on-esc="!saving"
    >
      <EditorForm form-id="password-reset-form" @submit="resetUserPassword">
        <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
          为 {{ resetPasswordTarget?.display_name }}（{{ resetPasswordTarget?.username }}）设置新密码。
        </p>
        <Input
          v-model="resetPasswordValue"
          label="新密码"
          type="password"
          autocomplete="new-password"
          required
        />
      </EditorForm>
      <template #footer>
        <Button :disabled="saving" @click="resetPasswordOpen = false">取消</Button>
        <Button
          form="password-reset-form"
          type="submit"
          variant="solid"
          semantic="error"
          icon="ph:key"
          :disabled="saving"
        >
          重置
        </Button>
      </template>
    </Modal>

    <Drawer
      v-model:visible="groupEditorOpen"
      :title="editingGroupId ? '编辑安全组' : '新建安全组'"
      :blocked="saving"
      :disabled="saving"
    >
      <GroupEditor
        v-model="groupForm"
        :items="treeItems"
        :editing-id="editingGroupId"
        :saving="saving"
        @save="saveGroup"
      />
      <template #footer>
        <Button :disabled="saving" @click="groupEditorOpen = false">取消</Button>
        <Button
          form="group-form"
          type="submit"
          variant="solid"
          semantic="primary"
          icon="ph:floppy-disk"
          :disabled="saving"
        >
          保存
        </Button>
      </template>
    </Drawer>
  </AdminShell>
</template>
