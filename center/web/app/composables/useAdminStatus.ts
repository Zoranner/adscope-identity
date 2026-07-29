let statusTimer: ReturnType<typeof setTimeout> | undefined

export function useAdminStatus() {
  const message = useState('admin-status-message', () => '')
  const isError = useState('admin-status-error', () => false)
  const visible = useState('admin-status-visible', () => false)

  function setStatus(nextMessage: string, error = false) {
    if (statusTimer) {
      clearTimeout(statusTimer)
    }
    message.value = nextMessage
    isError.value = error
    visible.value = nextMessage.length > 0

    if (import.meta.client && nextMessage.length > 0) {
      statusTimer = setTimeout(() => {
        visible.value = false
      }, error ? 5000 : 3000)
    }
  }

  function clearStatus() {
    if (statusTimer) {
      clearTimeout(statusTimer)
    }
    visible.value = false
  }

  return {
    message,
    isError,
    visible,
    setStatus,
    clearStatus,
  }
}
