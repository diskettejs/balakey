import { rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect } from 'vitest'
import { FileSet, type Progress, type ProgressEvent, type StartEvent } from '../index.js'
import { capture, test } from './support.ts'

const fixturesDir = fileURLToPath(import.meta.resolve('./fixtures'))

describe('FileSet', () => {
  describe('from()', () => {
    test('expands a glob into a path snapshot', async () => {
      const fileSet = await FileSet.from(fixturesDir, '**/*')

      expect(fileSet.paths.length).toBeGreaterThan(0)

      const results = await Array.fromAsync(fileSet.hash())
      expect(results.map((r) => r.path)).toEqualPaths(fileSet.paths)
    })

    test('expands brace alternation in a single pattern', async ({ dir, write, at }) => {
      await write({ 'a.mp4': 'x', 'b.mkv': 'y', 'c.txt': 'z' })

      const fileSet = await FileSet.from(dir, '*.{mp4,mkv}')
      expect(fileSet.paths).toEqualPaths(at('a.mp4', 'b.mkv'))
    })

    test('unions and dedupes multiple patterns', async ({ dir, write, at }) => {
      await write({ 'a.mp4': 'x', 'b.mkv': 'y' })

      // a.mp4 matches both '*.mp4' and '**/*.mp4' but should appear once
      const fileSet = await FileSet.from(dir, ['*.mp4', '*.mkv', '**/*.mp4'])
      expect(fileSet.paths).toEqualPaths(at('a.mp4', 'b.mkv'))
    })

    test('excludes matching subtrees via ignore', async ({ dir, write, at }) => {
      await write({ 'keep/a.txt': 'a', 'skip/b.txt': 'b' })

      const fileSet = await FileSet.from(dir, '**/*.txt', { ignore: ['skip/**'] })
      expect(fileSet.paths).toEqualPaths(at('keep/a.txt'))
    })

    test('excludes directories from results', async ({ dir, write, at }) => {
      await write({ 'nested/file.txt': 'x' })

      const fileSet = await FileSet.from(dir, '**/*')
      expect(fileSet.paths).toEqualPaths(at('nested/file.txt'))
    })

    test('rejects when the root is not a directory', async () => {
      await expect(FileSet.from(join(tmpdir(), 'balakey-does-not-exist'), '**/*')).rejects.toThrow()
    })

    test('rejects an invalid glob pattern', async ({ dir }) => {
      await expect(FileSet.from(dir, '***')).rejects.toThrow()
    })
  })

  describe('hash()', () => {
    test('can run multiple times', async () => {
      const fileSet = await FileSet.from(fixturesDir, '**/*')

      const [first, second] = await Promise.all([
        Array.fromAsync(fileSet.hash()),
        Array.fromAsync(fileSet.hash()),
      ])
      expect(first.length).toBe(second.length)
    })

    test('yields a failed entry without aborting the stream', async ({ dir, write }) => {
      await write({ 'kept.bin': 'kept', 'gone.bin': 'gone' })

      const fileSet = await FileSet.from(dir, '*.bin')
      expect(fileSet.paths).toHaveLength(2)

      // paths are snapshotted at expansion, so deleting a file now forces
      // an I/O failure during hash()
      await rm(join(dir, 'gone.bin'))

      const results = await Array.fromAsync(fileSet.hash())
      expect(results).toHaveLength(2)

      const ok = results.filter((r) => r.hashed)
      const failed = results.filter((r) => !r.hashed)
      expect(ok).toMatchObject([{ path: join(dir, 'kept.bin'), hashed: true }])
      expect(failed).toMatchObject([
        { path: join(dir, 'gone.bin'), hashed: false, error: expect.any(String) },
      ])
    })

    test('supports early termination', async () => {
      const fileSet = await FileSet.from(fixturesDir, '**/*')

      for await (const result of fileSet.hash()) {
        expect(result.path).toEqual(expect.any(String))
        break
      }
    })

    test('reports cumulative progress per entry', async ({ dir, write }) => {
      await write({ 'a.bin': 'a', 'b.bin': 'b', 'c.bin': 'c', 'd.bin': 'd', 'gone.bin': 'gone' })

      const fileSet = await FileSet.from(dir, '*.bin')
      const total = fileSet.paths.length
      expect(total).toBe(5)

      await rm(join(dir, 'gone.bin'))

      const results: Progress[] = await Array.fromAsync(fileSet.hash())
      expect(results).toHaveLength(total)

      results.forEach((entry, i) => {
        expect(entry.stats.total).toBe(total)
        // counts are stamped at the yield boundary, so each entry's tally equals
        // its 1-based position in the stream
        expect(entry.stats.hashed + entry.stats.failed).toBe(i + 1)

        if (entry.hashed) {
          expect(typeof entry.hash).toBe('string')
          expect(typeof entry.duration).toBe('number')
        } else {
          expect(typeof entry.error).toBe('string')
        }
      })

      const last = results.at(-1)!
      expect(last.stats.hashed).toBe(4)
      expect(last.stats.failed).toBe(1)
    })

    test('hashes an empty file', async ({ dir, write, at }) => {
      await write({ 'empty.bin': '' })

      const fileSet = await FileSet.from(dir, '*.bin')
      expect(fileSet.paths).toEqualPaths(at('empty.bin'))

      const entries: Progress[] = []

      for await (const entry of fileSet.hash()) {
        entries.push(entry)
      }

      expect(entries).toHaveLength(1)

      const [entry] = entries
      expect(entry).toMatchObject({
        path: join(dir, 'empty.bin'),
        hashed: true,
        // BLAKE3 digest of the empty input
        hash: 'af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262',
      })
      expect(entry?.stats).toMatchObject({ hashed: 1, failed: 0, total: 1 })
    })

    test('hashes an empty file with onProgress', async ({ dir, write, at }) => {
      await write({ 'empty.bin': '' })

      const fileSet = await FileSet.from(dir, '*.bin')
      expect(fileSet.paths).toEqualPaths(at('empty.bin'))

      const entries: Progress[] = []

      for await (const entry of fileSet.hash({ onProgress() {} })) {
        entries.push(entry)
      }

      expect(entries).toHaveLength(1)

      const [entry] = entries
      expect(entry).toMatchObject({
        path: join(dir, 'empty.bin'),
        hashed: true,
        // BLAKE3 digest of the empty input
        hash: 'af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262',
      })
      expect(entry?.stats).toMatchObject({ hashed: 1, failed: 0, total: 1 })
    })

    test('emits start and progress events per file', async ({ dir, write }) => {
      const contents: Record<string, string> = {
        'a.bin': 'a'.repeat(1000),
        'b.bin': 'b'.repeat(2000),
      }
      await write(contents)

      const fileSet = await FileSet.from(dir, '*.bin')

      const starts = capture<StartEvent>()
      const progresses = capture<ProgressEvent>()

      const results = await Array.fromAsync(
        fileSet.hash({ onStart: starts.push, onProgress: progresses.push }),
      )

      expect(results).toHaveLength(2)

      for (const [name, content] of Object.entries(contents)) {
        const path = join(dir, name)

        const start = starts.items.find((s) => s.path === path)
        expect(start?.size).toBe(content.length)

        const events = progresses.items.filter((p) => p.path === path)
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
    })

    test('emits start events without onProgress', async ({ dir, write }) => {
      await write({ 'only.bin': 'x'.repeat(500) })
      const fileSet = await FileSet.from(dir, '*.bin')

      const starts = capture<StartEvent>()
      const results = await Array.fromAsync(fileSet.hash({ onStart: starts.push }))

      expect(results).toHaveLength(1)
      expect(results[0]?.hashed).toBe(true)
      expect(starts.items).toMatchObject([{ path: join(dir, 'only.bin'), size: 500 }])
    })

    test('chunked digest matches the fast path', async ({ dir, write }) => {
      await write({ 'data.bin': 'hello world'.repeat(100_000) })
      const fileSet = await FileSet.from(dir, '*.bin')

      const [fast] = await Array.fromAsync(fileSet.hash())
      const [chunked] = await Array.fromAsync(fileSet.hash({ onProgress: () => {} }))

      if (!fast?.hashed || !chunked?.hashed) {
        throw new Error('expected both fileSet to hash successfully')
      }
      expect(chunked.hash).toBe(fast.hash)
    })
  })
})
