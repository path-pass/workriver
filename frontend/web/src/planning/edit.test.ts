// Editing an intent on its page: what Save sends, what a version changed, a word diff, and
// what the service's refusal of an edit from an older version says.
import { describe, expect, it } from 'vitest'
import { changesOf, conflictOf, editBody, wordDiff } from './edit'
import type { Intent } from './types'

const intent = (more: Partial<Intent> = {}): Intent => ({ id: 'i3', title: 'Search', goal: 'Find fast', note: 'First\n\nSecond', units: [], chores: [], version: 4, ...more }) as Intent

describe('what Save sends', () => {
  it('only the fields that changed, with the version they were edited from', () => {
    expect(editBody(intent(), { title: 'Search', goal: 'Find faster', note: 'First\n\nSecond' })).toEqual({ intent: 'i3', goal: 'Find faster', base: 4 })
    expect(editBody(intent(), { title: ' Find ', goal: 'Find fast', note: '' })).toEqual({ intent: 'i3', title: 'Find', note: '', base: 4 })
  })

  it('nothing when nothing changed (spaces at the ends are not a change) or the title is blank', () => {
    expect(editBody(intent(), { title: 'Search ', goal: 'Find fast', note: 'First\n\nSecond\n' })).toBeNull()
    expect(editBody(intent(), { title: '  ', goal: 'Other', note: '' })).toBeNull()
  })

  it('an intent the page has no version of is at v1; a missing goal or note is empty', () => {
    expect(editBody(intent({ version: undefined, goal: undefined, note: undefined }), { title: 'Search', goal: '', note: 'New' })).toEqual({ intent: 'i3', note: 'New', base: 1 })
  })

  it('a version picked to restore is sent from the newest, as a new edit', () => {
    expect(editBody(intent(), { title: 'Old', goal: 'Find fast', note: 'First' })).toEqual({ intent: 'i3', title: 'Old', note: 'First', base: 4 })
  })
})

describe('what a version changed', () => {
  it('in the order shown: created, merged, title, goal, note, an addition', () => {
    expect(changesOf({ n: 1, at: '', by: 'user', how: 'created', title: 'A', goal: '', note: '' })).toEqual(['created'])
    expect(changesOf({ n: 2, at: '', by: 'user', how: 'edited', title: 'B', note: 'x' })).toEqual(['title', 'note'])
    expect(changesOf({ n: 3, at: '', by: 's-boss', how: 'edited', added: 'More' })).toEqual(['added'])
    expect(changesOf({ n: 4, at: '', by: 'user', how: 'merged', merged: ['i7'], title: 'C' })).toEqual(['merged', 'title'])
  })
})

describe('a word diff', () => {
  it('keeps what stayed, and marks what was removed before what was added', () => {
    expect(wordDiff('Find messages fast', 'Find messages faster')).toEqual([
      { kind: 'same', text: 'Find messages ' },
      { kind: 'del', text: 'fast' },
      { kind: 'add', text: 'faster' },
    ])
    expect(wordDiff('', 'New')).toEqual([{ kind: 'add', text: 'New' }])
    expect(wordDiff('Same', 'Same')).toEqual([{ kind: 'same', text: 'Same' }])
  })

  it('splits Chinese and Japanese by their words, not by spaces they do not have', () => {
    const runs = wordDiff('这一轮先做登录限流。', '这一轮先做上传检查。')
    expect(runs.filter(r => r.kind === 'del').map(r => r.text).join('')).toBe('登录限流')
    expect(runs.filter(r => r.kind === 'add').map(r => r.text).join('')).toBe('上传检查')
    expect(wordDiff('ログインの制限を先に', 'アップロードの制限を先に')).toEqual([
      { kind: 'del', text: 'ログイン' },
      { kind: 'add', text: 'アップロード' },
      { kind: 'same', text: 'の制限を先に' },
    ])
  })

  it('both texts are what it says: the old one without the additions, the new one without the removals', () => {
    const runs = wordDiff('First paragraph.\n\nSecond one, long.', 'First paragraph, changed.\n\nSecond one.\n\nThird.')
    expect(runs.filter(r => r.kind !== 'add').map(r => r.text).join('')).toBe('First paragraph.\n\nSecond one, long.')
    expect(runs.filter(r => r.kind !== 'del').map(r => r.text).join('')).toBe('First paragraph, changed.\n\nSecond one.\n\nThird.')
  })
})

describe('a refusal of an edit from an older version', () => {
  it('names the field that changed and the version it is at now', () => {
    const said = "/api/append: 422 i3's note changed since v2 (in v3; it is at v5, last by claude 1 at 2026-11-20T17:50:58Z); read it (`workriver intent history i3 --version 3`) and run again with `--base 5`"
    expect(conflictOf(said)).toEqual({ field: 'note', now: 5 })
    expect(conflictOf("i3's title changed since v1 (in v2; it is at v2, last by the user at x)")).toEqual({ field: 'title', now: 2 })
  })

  it('anything else is not a conflict', () => {
    expect(conflictOf('/api/append: 422 an intent needs a title')).toBeNull()
  })
})
