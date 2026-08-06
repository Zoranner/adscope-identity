export type UserStatus = 'active' | 'disabled'

export type OAuthClientType = 'web' | 'desktop'

export type OAuthScope = 'openid' | 'profile' | 'email' | 'phone'

export interface OAuthClient {
  client_id: string
  name: string
  client_type: OAuthClientType
  redirect_uris: string[]
  allowed_scopes: OAuthScope[]
  enabled: boolean
}

export interface CreateOAuthClientRequest {
  name: string
  client_type: OAuthClientType
  redirect_uris: string[]
  allowed_scopes: OAuthScope[]
  enabled: boolean
}

export interface UpdateOAuthClientRequest {
  name: string
  redirect_uris: string[]
  allowed_scopes: OAuthScope[]
  enabled: boolean
}

export interface OAuthClientCreateResponse {
  client: OAuthClient
  client_secret: string | null
}

export type OAuthClientUpdateResponse = OAuthClient

export interface OAuthClientSecretResponse {
  client_id: string
  client_secret: string
}

export interface Domain {
  id: string
  name: string
  enabled: boolean
  mirror_root_dn: string
  quarantine_ou_dn: string
  upn_suffix: string
  employee_id_attribute: string
  managed_group_id_attribute: string
  applied_directory_revision: number
  applied_credential_revision: number
}

export interface DomainMutationResponse {
  domain: Domain
  connector_key: string
}

export interface OrganizationalUnit {
  id: string
  name: string
  parent_id: string | null
  changed_revision: number
}

export interface UserRecord {
  employee_id: string
  username: string
  display_name: string
  email: string | null
  mobile: string | null
  telephone: string | null
  organizational_unit_id: string
  status: UserStatus
}

export interface GroupRecord {
  id: string
  name: string
  organizational_unit_id: string
  member_employee_ids: string[]
  changed_revision: number
}

export interface SyncDomain {
  domain_id: string
  enabled: boolean
  applied_directory_revision: number
  applied_credential_revision: number
  directory_lag: number
  credential_lag: number
}

export interface DomainForm {
  id: string
  name: string
  enabled: boolean
  mirror_root_dn: string
  quarantine_ou_dn: string
  upn_suffix: string
  employee_id_attribute: string
  managed_group_id_attribute: string
}

export interface OuForm {
  id: string
  name: string
  parent_id: string
}

export interface UserForm {
  employee_id: string
  username: string
  display_name: string
  email: string
  mobile: string
  organizational_unit_id: string
  status: UserStatus
  initial_password: string
  reset_password: string
}

export interface GroupForm {
  id: string
  name: string
  organizational_unit_id: string
  member_employee_ids: string
}

export interface OuTreeItem {
  ou: OrganizationalUnit
  depth: number
  userCount: number
  groupCount: number
}
