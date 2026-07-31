let userStatusTimer: ReturnType<typeof setTimeout> | undefined

export function useUserStatus() {
  const message = useState('user-status-message', () => '')
  const isError = useState('user-status-error', () => false)
  const visible = useState('user-status-visible', () => false)

  function setStatus(nextMessage: string, error = false) {
    if (userStatusTimer) {
      clearTimeout(userStatusTimer)
    }
    message.value = nextMessage
    isError.value = error
    visible.value = nextMessage.length > 0

    if (import.meta.client && nextMessage.length > 0) {
      userStatusTimer = setTimeout(() => {
        visible.value = false
      }, error ? 5000 : 3000)
    }
  }

  function clearStatus() {
    if (userStatusTimer) {
      clearTimeout(userStatusTimer)
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
