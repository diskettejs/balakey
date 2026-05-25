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
      hash: expect.any(String),
      path: `${fixturesDir}/hello-world.txt`,
      duration: expect.any(Number),
    },
  ])
})
