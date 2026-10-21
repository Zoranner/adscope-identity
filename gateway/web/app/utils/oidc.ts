import type { OidcAuthorization, OidcFormField } from '../types/oidc'

const authorizationPath = '/oauth2/authorize'

export function resolveAuthorizationContinue(value: unknown, origin: string): string | null {
  if (typeof value !== 'string' || value.length === 0) {
    return null
  }

  try {
    const url = new URL(value, `${origin}/`)
    if (url.origin !== origin || url.pathname !== authorizationPath) {
      return null
    }
    return `${url.pathname}${url.search}`
  } catch {
    return null
  }
}

export function authorizationFormFields(
  authorization: OidcAuthorization,
): OidcFormField[] {
  const fields: OidcFormField[] = [
    { name: 'response_type', value: authorization.response_type },
    { name: 'client_id', value: authorization.client_id },
    { name: 'redirect_uri', value: authorization.redirect_uri },
    { name: 'scope', value: authorization.scope },
    { name: 'state', value: authorization.state },
    { name: 'nonce', value: authorization.nonce },
    { name: 'code_challenge', value: authorization.code_challenge },
    { name: 'code_challenge_method', value: authorization.code_challenge_method },
  ]

  if (authorization.response_mode !== undefined && authorization.response_mode !== null) {
    fields.push({ name: 'response_mode', value: authorization.response_mode })
  }
  if (authorization.prompt !== undefined && authorization.prompt !== null) {
    fields.push({ name: 'prompt', value: authorization.prompt })
  }

  return fields
}
