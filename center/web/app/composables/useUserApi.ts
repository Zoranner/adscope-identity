import type {
  PasswordChangeResponse,
  UserContactUpdateResponse,
  UserLoginResponse,
  UserProfile,
} from '~/types/user'

export function useUserApi() {
  const accessToken = useState('user-access-token', () => '')
  const profile = useState<UserProfile | null>('user-profile', () => null)
  const loading = useState('user-loading', () => false)
  const tokenReady = computed(() => accessToken.value.trim().length > 0)
  const { setStatus } = useUserStatus()

  function loadToken(): string {
    if (!import.meta.client) {
      return ''
    }
    const storedToken = window.localStorage.getItem('adscope.userAccessToken') ?? ''
    accessToken.value = storedToken
    return storedToken
  }

  function clearToken() {
    accessToken.value = ''
    profile.value = null
    if (import.meta.client) {
      window.localStorage.removeItem('adscope.userAccessToken')
    }
  }

  async function logout(showMessage = true) {
    try {
      await fetch('/api/auth/logout', { method: 'POST' })
    } catch {
      // Local credentials must still be cleared when the server cannot be reached.
    } finally {
      clearToken()
      if (showMessage) {
        setStatus('已退出登录')
      }
    }
  }

  async function userFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
    if (!tokenReady.value) {
      throw new Error('请先登录')
    }

    const response = await fetch(path, {
      ...init,
      headers: {
        authorization: `Bearer ${accessToken.value.trim()}`,
        ...(init.body ? { 'content-type': 'application/json' } : {}),
        ...init.headers,
      },
    })

    if (!response.ok) {
      if (response.status === 401 || response.status === 403) {
        clearToken()
      }
      throw new Error(
        response.status === 401 || response.status === 403
          ? '登录已失效，请重新登录'
          : `${response.status} ${response.statusText}`,
      )
    }

    return (await response.json()) as T
  }

  async function login(username: string, password: string): Promise<boolean> {
    const trimmedUsername = username.trim()
    if (!trimmedUsername || !password) {
      setStatus('请输入用户名和密码', true)
      return false
    }

    loading.value = true
    try {
      const response = await fetch('/api/auth/login', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          username: trimmedUsername,
          password,
        }),
      })

      if (!response.ok) {
        throw new Error(
          response.status === 401 || response.status === 403
            ? '用户名或密码错误'
            : `${response.status} ${response.statusText}`,
        )
      }

      const payload = (await response.json()) as UserLoginResponse
      accessToken.value = payload.access_token
      if (import.meta.client) {
        window.localStorage.setItem('adscope.userAccessToken', payload.access_token)
      }
      const profileLoaded = await loadMe(false)
      if (!profileLoaded) {
        throw new Error('登录成功，但无法读取用户资料')
      }
      setStatus('已登录')
      return true
    } catch (error) {
      clearToken()
      setStatus(error instanceof Error ? error.message : '登录失败', true)
      return false
    } finally {
      loading.value = false
    }
  }

  async function loadMe(showError = true): Promise<boolean> {
    loading.value = true
    try {
      profile.value = await userFetch<UserProfile>('/api/me')
      return true
    } catch (error) {
      clearToken()
      if (showError) {
        setStatus(error instanceof Error ? error.message : '无法读取用户资料', true)
      }
      return false
    } finally {
      loading.value = false
    }
  }

  async function updateContact(email: string | null, mobile: string | null): Promise<boolean> {
    loading.value = true
    try {
      const response = await userFetch<UserContactUpdateResponse>('/api/me/contact', {
        method: 'PATCH',
        body: JSON.stringify({
          email,
          mobile,
          telephone: profile.value?.telephone ?? null,
        }),
      })
      profile.value = response.profile
      setStatus('联系方式已保存')
      return true
    } catch (error) {
      setStatus(error instanceof Error ? error.message : '联系方式保存失败', true)
      return false
    } finally {
      loading.value = false
    }
  }

  async function changePassword(currentPassword: string, newPassword: string): Promise<boolean> {
    if (!currentPassword || !newPassword) {
      setStatus('请输入当前密码和新密码', true)
      return false
    }

    loading.value = true
    try {
      await userFetch<PasswordChangeResponse>('/api/me/password', {
        method: 'POST',
        body: JSON.stringify({
          current_password: currentPassword,
          new_password: newPassword,
        }),
      })
      setStatus('密码已修改')
      return true
    } catch (error) {
      setStatus(error instanceof Error ? error.message : '密码修改失败', true)
      return false
    } finally {
      loading.value = false
    }
  }

  return {
    accessToken,
    tokenReady,
    loading,
    profile,
    loadToken,
    clearToken,
    logout,
    login,
    loadMe,
    updateContact,
    changePassword,
  }
}
