// The UI does not know the language of content; a guess picks the right CJK glyphs. (That
// the three dictionaries have the same keys is held by `tsc -b`, plural forms included.)
import { describe, expect, it } from 'vitest'
import { contentLang } from './index'

describe('the language of content', () => {
  it('kana is Japanese, Han without kana Chinese, anything else unset', () => {
    expect(contentLang('ひらがな')).toBe('ja')
    expect(contentLang('カタカナと漢字')).toBe('ja')
    expect(contentLang('只有汉字')).toBe('zh-CN')
    expect(contentLang('Mixed 中文 text')).toBe('zh-CN')
    expect(contentLang('plain text')).toBeUndefined()
    expect(contentLang('')).toBeUndefined()
    expect(contentLang(null)).toBeUndefined()
    expect(contentLang(undefined)).toBeUndefined()
  })
})
