import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'
import { FileSet, type HashError, type HashResult } from '../index.js'

const isOk = (entry: HashResult | HashError): entry is HashResult => !('error' in entry)

const fixturesDir = fileURLToPath(import.meta.resolve('./fixtures'))

test('FileSet.paths returns the expanded glob snapshot', async () => {
  const files = new FileSet([`${fixturesDir}/**/*`])

  expect(files.paths.length).toBeGreaterThan(0)

  const results = await Array.fromAsync(files.hash())
  expect(results.map((r) => r.path).toSorted()).toEqual(files.paths.toSorted())
})

test('FileSet.hash() can run multiple times', async () => {
  const files = new FileSet([`${fixturesDir}/**/*`])

  const [first, second] = await Promise.all([
    Array.fromAsync(files.hash()),
    Array.fromAsync(files.hash()),
  ])
  expect(first.length).toBe(second.length)
})

test('FileSet.hash() yields HashError for unreadable files without aborting', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'balakey-'))
  try {
    await writeFile(join(dir, 'kept.bin'), 'kept')
    await writeFile(join(dir, 'gone.bin'), 'gone')

    const files = new FileSet([`${dir}/*.bin`])
    expect(files.paths).toHaveLength(2)

    // paths are snapshotted at construction, so deleting a file now forces
    // an I/O failure during hash()
    await rm(join(dir, 'gone.bin'))

    const results = await Array.fromAsync(files.hash())
    expect(results).toHaveLength(2)

    const ok = results.filter(isOk)
    const failed = results.filter((r) => !isOk(r))
    expect(ok).toMatchObject([{ path: join(dir, 'kept.bin') }])
    expect(failed).toMatchObject([{ path: join(dir, 'gone.bin'), error: expect.any(String) }])
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})

test('FileSet.hash() supports early termination', async () => {
  const files = new FileSet([`${fixturesDir}/**/*`])

  for await (const result of files.hash()) {
    expect(result.path).toEqual(expect.any(String))
    break
  }
})
