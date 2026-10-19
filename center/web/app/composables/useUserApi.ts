import { useNotification } from '@stellar/ui'
import type {
  PasswordChangeResponse,
  UserContactUpdateResponse,
  UserLoginResponse,
  UserProfile,
} from '~/types/user'

function requestErrorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback
}

export function useUserApi() {
  const notifications = useNotification()
  const accessToken = useState('user-access-token', () => '')
  const profile = useState<UserProfile | null>('user-profile', () => null)
  const pending = useState('user-pending', () => 0)
  const tokenReady = computed(() => accessToken.value.trim().length > 0)
  const loading = computed(() => pending.value > 0)

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

  async function trackRequest<T>(request: () => Promise<T>): Promise<T> {
    pending.value += 1
    try {
      return await request()
    } finally {
      pending.value -= 1
    }
  }

  async function logout(showMessage = true) {
    try {
      await trackRequest(() => fetch('/api/auth/logout', { method: 'POST' }))
    } catch {
      // 服务端不可达时仍需清理本地登录状态。
    } finally {
      clearToken()
      if (showMessage) {
        notifications.success('已退出登录')
      }
    }
  }

  async function userFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
    if (!tokenReady.value) {
      throw new Error('请先登录')
    }

    return await trackRequest(async () => {
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
    })
  }

  async function login(username: string, password: string): Promise<boolean> {
    const trimmedUsername = username.trim()
    if (!trimmedUsername || !password) {
      notifications.error('请输入用户名和密码')
      return false
    }

    try {
      const payload = await trackRequest(async () => {
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

        return (await response.json()) as UserLoginResponse
      })

      accessToken.value = payload.access_token
      if (import.meta.client) {
        window.localStorage.setItem('adscope.userAccessToken', payload.access_token)
      }
      const profileLoaded = await loadMe(false)
      if (!profileLoaded) {
        throw new Error('登录成功，但无法读取用户资料')
      }
      notifications.success('已登录')
      return true
    } catch (error) {
      clearToken()
      notifications.error(requestErrorMessage(error, '登录失败'))
      return false
    }
  }

  async function loadMe(showError = true): Promise<boolean> {
    try {
      profile.value = await userFetch<UserProfile>('/api/me')
      return true
    } catch (error) {
      clearToken()
      if (showError) {
        notifications.error(requestErrorMessage(error, '无法读取用户资料'))
      }
      return false
    }
  }

  async function updateContact(email: string | null, mobile: string | null): Promise<boolean> {
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
      notifications.success('联系方式已保存')
      return true
    } catch (error) {
      notifications.error(requestErrorMessage(error, '联系方式保存失败'))
      return false
    }
  }

  async function changePassword(currentPassword: string, newPassword: string): Promise<boolean> {
    if (!currentPassword || !newPassword) {
      notifications.error('请输入当前密码和新密码')
      return false
    }

    try {
      await userFetch<PasswordChangeResponse>('/api/me/password', {
        method: 'POST',
        body: JSON.stringify({
          current_password: currentPassword,
          new_password: newPassword,
        }),
      })
      notifications.success('密码已修改')
      return true
    } catch (error) {
      notifications.error(requestErrorMessage(error, '密码修改失败'))
      return false
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
