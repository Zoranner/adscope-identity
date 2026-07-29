import type { DomainForm, GroupForm, OuForm, UserForm } from '~/types/admin'

export function blankDomainForm(): DomainForm {
  return {
    id: '',
    name: '',
    enabled: true,
    mirror_root_dn: '',
    quarantine_ou_dn: '',
    upn_suffix: '',
    employee_id_attribute: 'employeeID',
    managed_group_id_attribute: 'adminDescription',
    connector_key: '',
  }
}

export function blankOuForm(): OuForm {
  return {
    id: '',
    name: '',
    parent_id: '',
  }
}

export function blankUserForm(): UserForm {
  return {
    employee_id: '',
    username: '',
    display_name: '',
    email: '',
    mobile: '',
    telephone: '',
    organizational_unit_id: '',
    status: 'active',
    initial_password: '',
    reset_password: '',
  }
}

export function blankGroupForm(): GroupForm {
  return {
    id: '',
    name: '',
    organizational_unit_id: '',
    member_employee_ids: '',
  }
}

export function nullable(value: string): string | null {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

export function splitMembers(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
}
