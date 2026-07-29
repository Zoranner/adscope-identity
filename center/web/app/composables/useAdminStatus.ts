export function useAdminStatus() {
  const message = useState('admin-status-message', () => '')
  const isError = useState('admin-status-error', () => false)

  function setStatus(nextMessage: string, error = false) {
    message.value = nextMessage
    isError.value = error
  }

  return {
    message,
    isError,
    setStatus,
  }
}
