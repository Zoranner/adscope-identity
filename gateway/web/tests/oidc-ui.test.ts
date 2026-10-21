import { describe, expect, test } from 'bun:test'
import { authorizationFormFields, resolveAuthorizationContinue } from '../app/utils/oidc'

describe('resolveAuthorizationContinue', () => {
  const origin = 'https://center.example.test'

  test('accepts relative authorization URLs and returns an internal path', () => {
    expect(
      resolveAuthorizationContinue(
        '/oauth2/authorize?client_id=desktop&state=state-value',
        origin,
      ),
    ).toBe('/oauth2/authorize?client_id=desktop&state=state-value')
    expect(resolveAuthorizationContinue('oauth2/authorize?client_id=desktop', origin)).toBe(
      '/oauth2/authorize?client_id=desktop',
    )
  })

  test('accepts an absolute authorization URL only when it has the same origin', () => {
    expect(
      resolveAuthorizationContinue(
        'https://center.example.test/oauth2/authorize?client_id=desktop',
        origin,
      ),
    ).toBe('/oauth2/authorize?client_id=desktop')
    expect(
      resolveAuthorizationContinue(
        'https://client.example.test/oauth2/authorize?client_id=desktop',
        origin,
      ),
    ).toBeNull()
  })

  test('requires the exact authorization pathname', () => {
    expect(resolveAuthorizationContinue('/oauth2/authorize/callback?state=value', origin)).toBeNull()
    expect(resolveAuthorizationContinue('/authorize?state=value', origin)).toBeNull()
    expect(resolveAuthorizationContinue('/oauth2%2Fauthorize?state=value', origin)).toBeNull()
  })

  test('rejects non-string and non-http URL values', () => {
    expect(resolveAuthorizationContinue(undefined, origin)).toBeNull()
    expect(resolveAuthorizationContinue(['/oauth2/authorize'], origin)).toBeNull()
    expect(resolveAuthorizationContinue('javascript:alert(1)', origin)).toBeNull()
  })

  test('drops URL fragments from accepted authorization requests', () => {
    expect(resolveAuthorizationContinue('/oauth2/authorize?state=value#ignored', origin)).toBe(
      '/oauth2/authorize?state=value',
    )
  })
})

describe('authorizationFormFields', () => {
  const authorization = {
    response_type: 'code',
    client_id: 'desktop-client',
    redirect_uri: 'http://127.0.0.1:47123/callback',
    scope: 'openid profile email',
    state: 'state-value',
    nonce: 'nonce-value',
    code_challenge: 'challenge-value',
    code_challenge_method: 'S256',
    response_mode: null,
    prompt: null,
  }

  test('converts the server-confirmed authorization request into form fields', () => {
    expect(authorizationFormFields(authorization)).toEqual([
      { name: 'response_type', value: 'code' },
      { name: 'client_id', value: 'desktop-client' },
      { name: 'redirect_uri', value: 'http://127.0.0.1:47123/callback' },
      { name: 'scope', value: 'openid profile email' },
      { name: 'state', value: 'state-value' },
      { name: 'nonce', value: 'nonce-value' },
      { name: 'code_challenge', value: 'challenge-value' },
      { name: 'code_challenge_method', value: 'S256' },
    ])
  })

  test('includes confirmed optional fields and omits null optional fields', () => {
    expect(
      authorizationFormFields({
        ...authorization,
        response_mode: 'query',
        prompt: 'none',
      }).slice(-2),
    ).toEqual([
      { name: 'response_mode', value: 'query' },
      { name: 'prompt', value: 'none' },
    ])
    expect(
      authorizationFormFields({
        ...authorization,
        response_mode: null,
        prompt: null,
      }),
    ).toHaveLength(8)
  })
})
