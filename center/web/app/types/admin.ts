export type UserStatus = 'active' | 'disabled'

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
  connector_key: string
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
