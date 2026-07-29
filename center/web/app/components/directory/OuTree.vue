<script setup lang="ts">
import { FolderTree } from 'lucide-vue-next'
import type { OuTreeItem } from '~/types/admin'

defineProps<{
  items: OuTreeItem[]
  selectedId: string | null
  disabled?: boolean
}>()

defineEmits<{
  select: [id: string]
  create: [parentId: string | null]
}>()
</script>

<template>
  <section class="panel tree-panel">
    <div class="panel-header">
      <h2>OU 树</h2>
    </div>

    <div v-if="items.length" class="ou-tree">
      <button
        v-for="item in items"
        :key="item.ou.id"
        class="ou-tree-button"
        :class="{ active: selectedId === item.ou.id }"
        :style="{ paddingLeft: `${12 + item.depth * 16}px` }"
        @click="$emit('select', item.ou.id)"
      >
        <FolderTree :size="16" />
        <span class="ou-tree-name">{{ item.ou.name }}</span>
        <span class="ou-tree-count">{{ item.userCount }} / {{ item.groupCount }}</span>
      </button>
    </div>

    <AdminEmptyState
      v-else
      title="暂无组织单元"
      :action-label="disabled ? undefined : '新建 OU'"
      @action="$emit('create', null)"
    />
  </section>
</template>
