<script setup lang="ts">
import { ChevronDown, FolderTree, Plus, ShieldCheck, Users } from 'lucide-vue-next'

const props = defineProps<{
  disabled?: boolean
}>()

const emit = defineEmits<{
  createOu: []
  createUser: []
  createGroup: []
}>()
const root = ref<HTMLElement | null>(null)
const open = ref(false)

function toggle() {
  if (props.disabled) {
    return
  }
  open.value = !open.value
}

function close() {
  open.value = false
}

function choose(action: 'ou' | 'user' | 'group') {
  if (props.disabled) {
    return
  }
  close()
  if (action === 'ou') {
    emit('createOu')
  } else if (action === 'user') {
    emit('createUser')
  } else {
    emit('createGroup')
  }
}

function onDocumentClick(event: MouseEvent) {
  if (!root.value?.contains(event.target as Node)) {
    close()
  }
}

onMounted(() => document.addEventListener('click', onDocumentClick))
onBeforeUnmount(() => document.removeEventListener('click', onDocumentClick))
</script>

<template>
  <div ref="root" class="create-menu" @keydown.esc.stop.prevent="close">
    <button
      type="button"
      class="primary-button create-menu-trigger"
      :disabled="disabled"
      aria-haspopup="menu"
      :aria-expanded="open"
      @click="toggle"
    >
      <Plus :size="16" />
      新建
      <ChevronDown :size="15" />
    </button>
    <div v-if="open" class="create-menu-list" role="menu">
      <button type="button" @click="choose('ou')">
        <FolderTree :size="16" />
        子 OU
      </button>
      <button type="button" @click="choose('user')">
        <Users :size="16" />
        用户
      </button>
      <button type="button" @click="choose('group')">
        <ShieldCheck :size="16" />
        安全组
      </button>
    </div>
  </div>
</template>
