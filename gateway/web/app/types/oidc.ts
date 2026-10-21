export interface OidcAuthorization {
  response_type: string
  client_id: string
  redirect_uri: string
  scope: string
  state: string
  nonce: string
  code_challenge: string
  code_challenge_method: string
  response_mode: string | null
  prompt: string | null
}

export interface OidcAuthorizationUser {
  employee_id: string
  username: string
  display_name: string
}

export interface OidcAuthorizationContext {
  client_name: string
  user: OidcAuthorizationUser
  claims: Record<string, unknown>
  csrf_token: string
  authorization: OidcAuthorization
}

export interface OidcFormField {
  name: string
  value: string
}
