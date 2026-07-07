import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'
import { FileSet, type Progress, type ProgressEvent, type StartEvent } from '../index.js'

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

test('hash({ onStart, onProgress }) emits sub-file events per file', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'balakey-'))
  try {
    const contents: Record<string, string> = {
      'a.bin': 'a'.repeat(1000),
      'b.bin': 'b'.repeat(2000),
    }
    for (const [name, content] of Object.entries(contents)) {
      await writeFile(join(dir, name), content)
    }

    const files = new FileSet([`${dir}/*.bin`])

    const starts: StartEvent[] = []
    const progresses: ProgressEvent[] = []

    const results = await Array.fromAsync(
      files.hash({
        onStart: (e) => starts.push(e),
        onProgress: (e) => progresses.push(e),
      }),
    )

    expect(results).toHaveLength(2)

    for (const [name, content] of Object.entries(contents)) {
      const path = join(dir, name)

      const start = starts.find((s) => s.path === path)
      expect(start?.size).toBe(content.length)

      const events = progresses.filter((p) => p.path === path)
      expect(events.length).toBeGreaterThan(0)

      let prev = 0
      for (const e of events) {
        expect(e.size).toBe(content.length)
        expect(e.bytes).toBeGreaterThanOrEqual(prev)
        expect(e.bytes).toBeLessThanOrEqual(e.size)
        prev = e.bytes
      }
      // the final progress event for a file accounts for every byte
      expect(events.at(-1)!.bytes).toBe(content.length)
    }
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})

test('hash({ onStart }) works without onProgress and still hashes', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'balakey-'))
  try {
    await writeFile(join(dir, 'only.bin'), 'x'.repeat(500))
    const files = new FileSet([`${dir}/*.bin`])

    const starts: StartEvent[] = []
    const results = await Array.fromAsync(files.hash({ onStart: (e) => starts.push(e) }))

    expect(results).toHaveLength(1)
    expect(results[0]?.hashed).toBe(true)
    expect(starts).toMatchObject([{ path: join(dir, 'only.bin'), size: 500 }])
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})

test('chunked hashing (onProgress) produces the same digest as the fast path', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'balakey-'))
  try {
    await writeFile(join(dir, 'data.bin'), 'hello world'.repeat(100_000))
    const files = new FileSet([`${dir}/*.bin`])

    const [fast] = await Array.fromAsync(files.hash())
    const [chunked] = await Array.fromAsync(files.hash({ onProgress: () => {} }))

    if (!fast?.hashed || !chunked?.hashed) {
      throw new Error('expected both files to hash successfully')
    }
    expect(chunked.hash).toBe(fast.hash)
  } finally {
    await rm(dir, { recursive: true, force: true })
  }
})
