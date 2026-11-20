// The run dialog flags commands that can destroy work for good (a heuristic, not a guard).
import { describe, expect, it } from 'vitest'
import { dangerous } from './Run'

describe('destructive commands are flagged', () => {
  it.each([
    'rm -rf build',
    'rm -r dir',
    'rm -f a.txt',
    'sudo rm -Rf /tmp/x',
    'rm --verbose -rf x',
    'git push --force',
    'git push origin main --force-with-lease',
    'git push -f origin main',
    'git push origin +main',
    'git reset --hard',
    'git reset HEAD~1 --hard',
    'git clean -fd',
    'git clean -x -f',
    'git branch -D feat/x',
    'mkfs.ext4 /dev/sdb1',
    'dd if=/dev/zero of=/dev/sda',
    'cargo test && rm -rf target',
  ])('%s', command => expect(dangerous(command)).toBe(true))

  it.each([
    'rm a.txt',
    'git push',
    'git push origin feat/x',
    'git reset HEAD~1',
    'git reset --soft HEAD~1',
    'git clean -n',
    'git branch -d feat/x',
    'cargo test',
    'npm run build',
    'dd if=a',
  ])('%s is not', command => expect(dangerous(command)).toBe(false))
})
