import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'
import { FileSet, type Progress } from '../index.js'

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

test('FileSet.hash() yields a failed entry for unreadable files without aborting', async () => {
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

    const ok = results.filter((r) => r.hashed)
    const failed = results.filter((r) => !r.hashed)
    expect(ok).toMatchObject([{ path: join(dir, 'kept.bin'), hashed: true }])
    expect(failed).toMatchObject([
      { path: join(dir, 'gone.bin'), hashed: false, error: expect.any(String) },
    ])
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

test('FileSet.hash() reports cumulative progress per entry', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'balakey-'))
  try {
    for (const name of ['a', 'b', 'c', 'd', 'gone']) {
      await writeFile(join(dir, `${name}.bin`), name)
    }

    const files = new FileSet([`${dir}/*.bin`])
    const total = files.paths.length
    expect(total).toBe(5)

    await rm(join(dir, 'gone.bin'))

    const results: Progress[] = await Array.fromAsync(files.hash())
    expect(results).toHaveLength(total)

    results.forEach((entry, i) => {
      expect(entry.total).toBe(total)
      // counts are stamped at the yield boundary, so each entry's tally equals
      // its 1-based position in the stream
      expect(entry.succeeded + entry.failed).toBe(i + 1)

      if (entry.hashed) {
        expect(typeof entry.hash).toBe('string')
        expect(typeof entry.duration).toBe('number')
      } else {
        expect(typeof entry.error).toBe('string')
      }
    })

    const last = results.at(-1)!
    expect(last.succeeded).toBe(4)
    expect(last.failed).toBe(1)
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})
