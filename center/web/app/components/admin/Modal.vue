<script setup lang="ts">
import { X } from 'lucide-vue-next'

const props = defineProps<{
  open: boolean
  title: string
  width?: 'medium' | 'wide'
  closeDisabled?: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const modalPanel = ref<HTMLElement | null>(null)
let previousActiveElement: HTMLElement | null = null
let inertBackground: HTMLElement | null = null
let backgroundWasInert = false
let focusLifecycle = 0

const focusableSelector = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

function requestClose() {
  if (!props.closeDisabled) {
    emit('close')
  }
}

function focusableElements(): HTMLElement[] {
  if (!modalPanel.value) {
    return []
  }
  return Array.from(modalPanel.value.querySelectorAll<HTMLElement>(focusableSelector)).filter(
    (element) => element.tabIndex >= 0,
  )
}

function setBackgroundInert() {
  inertBackground = document.querySelector<HTMLElement>('.app-shell')
  if (inertBackground) {
    backgroundWasInert = inertBackground.inert
    inertBackground.inert = true
  }
}

function restoreBackground() {
  if (inertBackground) {
    inertBackground.inert = backgroundWasInert
    inertBackground = null
  }
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    requestClose()
    return
  }
  if (event.key !== 'Tab' || !modalPanel.value) {
    return
  }

  const elements = focusableElements()
  if (elements.length === 0) {
    event.preventDefault()
    modalPanel.value.focus()
    return
  }

  const first = elements[0]
  const last = elements[elements.length - 1]
  const activeElement = document.activeElement
  if (event.shiftKey && (activeElement === first || !modalPanel.value.contains(activeElement))) {
    event.preventDefault()
    last?.focus()
  } else if (!event.shiftKey && (activeElement === last || !modalPanel.value.contains(activeElement))) {
    event.preventDefault()
    first?.focus()
  }
}

watch(
  () => props.open,
  async (open) => {
    if (!import.meta.client) {
      return
    }
    const lifecycle = ++focusLifecycle
    if (open) {
      previousActiveElement =
        document.activeElement instanceof HTMLElement ? document.activeElement : null
      setBackgroundInert()
      await nextTick()
      if (props.open && focusLifecycle === lifecycle) {
        const firstFocusable = focusableElements()[0] ?? modalPanel.value
        firstFocusable?.focus()
      }
      return
    }

    restoreBackground()
    const elementToRestore = previousActiveElement
    previousActiveElement = null
    await nextTick()
    if (!props.open && focusLifecycle === lifecycle) {
      elementToRestore?.focus()
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  focusLifecycle += 1
  restoreBackground()
  previousActiveElement?.focus()
  previousActiveElement = null
})
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="modal-backdrop" @click.self="requestClose" @keydown="handleKeydown">
      <section
        ref="modalPanel"
        class="modal-panel"
        :class="width ?? 'medium'"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
      >
        <header class="modal-header">
          <h2>{{ title }}</h2>
          <button class="icon-button" title="关闭" :disabled="closeDisabled" @click="requestClose">
            <X :size="18" />
          </button>
        </header>
        <div class="modal-body">
          <slot />
        </div>
      </section>
    </div>
  </Teleport>
</template>
