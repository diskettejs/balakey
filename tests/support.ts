import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { test as base, expect } from 'vitest'

type WriteFiles = (files: Record<string, string>) => Promise<void>
type AtPaths = (...names: string[]) => string[]

export const test = base.extend<{ dir: string; write: WriteFiles; at: AtPaths }>({
  dir: async ({}, use) => {
    const dir = await mkdtemp(join(tmpdir(), 'balakey-'))
    await use(dir)
    await rm(dir, { recursive: true, force: true })
  },
  write: async ({ dir }, use) => {
    await use(async (files) => {
      for (const [rel, content] of Object.entries(files)) {
        const abs = join(dir, rel)
        await mkdir(dirname(abs), { recursive: true })
        await writeFile(abs, content)
      }
    })
  },
  at: async ({ dir }, use) => {
    await use((...names) => names.map((name) => join(dir, name)))
  },
})

expect.extend({
  toEqualPaths(received: string[], expected: string[]) {
    const actual = [...received].sort()
    const wanted = [...expected].sort()
    const pass = this.equals(actual, wanted)
    return {
      pass,
      message: () =>
        pass ? 'expected paths not to match' : `paths differ:\n${this.utils.diff(wanted, actual)}`,
    }
  },
})

declare module 'vitest' {
  interface Matchers<T = any> {
    toEqualPaths: (expected: string[]) => T
  }
}

export function capture<T>() {
  const items: T[] = []
  return { items, push: (value: T) => void items.push(value) }
}
