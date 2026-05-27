import { fileURLToPath } from 'node:url'
import { expect, test } from 'vitest'
import { hasher } from '../index.js'

const fixturesDir = fileURLToPath(import.meta.resolve('./fixtures'))

// TODO: improve tests
test('Balakey', async () => {
  const glob = `${fixturesDir}/**/*`
  const walker = hasher([glob])

  const results = await Array.fromAsync(walker)
  expect(results).toMatchSnapshot([
    {
      hash: expect.any(String),
      path: expect.any(String),
      duration: expect.any(Number),
    },
  ])
})
