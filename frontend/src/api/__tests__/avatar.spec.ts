import { describe, expect, it } from 'vitest'
import { avatarSrc } from '../avatar'

describe('avatarSrc', () => {
  it('puts the picture version in the URL and has no URL without one', () => {
    expect(avatarSrc('user-1', 1792300000000)).toBe('/api/avatars/user-1?v=1792300000000')
    expect(avatarSrc('user-1', null)).toBeNull()
    expect(avatarSrc('user-1', undefined)).toBeNull()
    expect(avatarSrc(null, 5)).toBeNull()
    expect(avatarSrc('', 5)).toBeNull()
  })
  it('never lets an id change the path', () => {
    expect(avatarSrc('a/../b', 1)).toBe('/api/avatars/a%2F..%2Fb?v=1')
  })
})
