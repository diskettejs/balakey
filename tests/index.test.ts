import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'
import { hasher } from '../index.js'

const fixturesDir = fileURLToPath(import.meta.resolve('./fixtures'))

// TODO: improve tests
test('Balakey', async () => {
  const glob = `${fixturesDir}/**/*`
  const balakey = hasher([glob])

  const results = await Array.fromAsync(balakey)
  expect(results).toMatchInlineSnapshot([
    {
      hash: 'dc5a4edb8240b018124052c330270696f96771a63b45250a5c17d3000e823355',
      path: `${fixturesDir}/hello-world.txt`,
      duration: expect.any(Number),
    },
  ])
})
