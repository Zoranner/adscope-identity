import type { UserStatus } from '~/types/admin'

export interface UserProfile {
  employee_id: string
  username: string
  display_name: string
  email: string | null
  mobile: string | null
  telephone: string | null
  organizational_unit_id: string
  status: UserStatus
}

export interface UserLoginResponse {
  employee_id: string
  access_token: string
}

export interface UserContactUpdateResponse {
  profile: UserProfile
  directory_revision: number
}

export interface PasswordChangeResponse {
  employee_id: string
  credential_revision: number
}
