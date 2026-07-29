import type { Ref } from 'vue'

export function usePagination<T>(items: Ref<T[]>, pageSize = 10) {
  const page = ref(1)
  const size = ref(pageSize)
  const total = computed(() => items.value.length)
  const pageCount = computed(() => Math.max(1, Math.ceil(total.value / size.value)))
  const start = computed(() => (page.value - 1) * size.value)
  const end = computed(() => Math.min(start.value + size.value, total.value))
  const pageItems = computed(() => items.value.slice(start.value, end.value))

  watch([total, size], () => {
    if (page.value > pageCount.value) {
      page.value = pageCount.value
    }
  })

  function nextPage() {
    page.value = Math.min(page.value + 1, pageCount.value)
  }

  function previousPage() {
    page.value = Math.max(page.value - 1, 1)
  }

  function resetPage() {
    page.value = 1
  }

  return {
    page,
    size,
    total,
    pageCount,
    start,
    end,
    pageItems,
    nextPage,
    previousPage,
    resetPage,
  }
}
