import test from 'node:test'
import assert from 'node:assert/strict'
import type { ChatModel } from '$lib/types/backend'
import { joinSelector, levelsForSelector, modelForSelector, splitSelector, THINKING_LEVELS } from './selector'

test('effort and tier suffix orders resolve alike and join canonically', () => {
  const parts = { model: 'gpt-x', effort: 'high', tier: 'fast' } as const
  assert.deepEqual(splitSelector('gpt-x:high:fast'), parts)
  assert.deepEqual(splitSelector('gpt-x:fast:high'), parts)
  assert.equal(joinSelector(parts.model, parts.effort, parts.tier), 'gpt-x:high:fast')
  const role = splitSelector('@review:flex:xhigh')!
  assert.deepEqual(role, { model: '@review', effort: 'xhigh', tier: 'flex' })
  assert.equal(joinSelector(role.model, role.effort, role.tier), '@review:xhigh:flex')
})

test('suffix-only selectors and explicit Standard retain their meaning', () => {
  assert.deepEqual(splitSelector(''), { model: '', effort: null, tier: null })
  assert.deepEqual(splitSelector('gpt-x'), { model: 'gpt-x', effort: null, tier: null })
  assert.deepEqual(splitSelector('@review'), { model: '@review', effort: null, tier: null })
  assert.deepEqual(splitSelector(':fast'), { model: '', effort: null, tier: 'fast' })
  assert.deepEqual(splitSelector(':high:fast'), { model: '', effort: 'high', tier: 'fast' })
  assert.deepEqual(splitSelector(':standard'), { model: '', effort: null, tier: 'standard' })
  assert.equal(joinSelector('', null, 'fast'), ':fast')
  assert.equal(joinSelector('gpt-x', null, 'standard'), 'gpt-x:standard')
  assert.equal(joinSelector('gpt-x', null, null), 'gpt-x')
})

test('duplicate, unknown, empty and case-folded suffixes are invalid', () => {
  for (const selector of [
    'gpt-x:low:high',
    'gpt-x:flex:fast',
    'gpt-x:sprint',
    'gpt-x::fast',
    'gpt-x:high:',
    ':',
    'gpt-x:Fast',
    '@review:high:high',
    ':fast:standard'
  ]) {
    assert.equal(splitSelector(selector), null, selector)
  }
})

const model: ChatModel = {
  selector: 'p/m',
  provider: 'p',
  origin: null,
  name: 'Model',
  thinking: ['high'],
  serviceTiers: { fast: 2 },
  contextWindow: 1,
  images: false
}
const colonModel: ChatModel = { ...model, selector: 'p/m:fast' }
const models = [model, colonModel]

test('catalog model IDs take precedence over suffix parsing, including when followed by suffixes', () => {
  assert.deepEqual(splitSelector('p/m:fast', models), { model: 'p/m:fast', effort: null, tier: null })
  assert.deepEqual(splitSelector('p/m:fast:high', models), { model: 'p/m:fast', effort: 'high', tier: null })
  assert.deepEqual(splitSelector('p/m:high:fast', models), { model: 'p/m', effort: 'high', tier: 'fast' })
  assert.deepEqual(splitSelector('p/m:fast:xhigh:ultrafast', models), {
    model: 'p/m:fast',
    effort: 'xhigh',
    tier: 'ultrafast'
  })
  assert.deepEqual(splitSelector('p/custom:ultrafast', models), { model: 'p/custom', effort: null, tier: 'ultrafast' })
})

test('model lookup follows a single role target and falls back when the model is unknown', () => {
  const roles = [
    { name: 'review', model: 'p/m:fast:xhigh' },
    { name: 'nested', model: '@review' },
    { name: 'tier-only', model: ':flex' }
  ]
  assert.equal(modelForSelector('@review:standard', models, roles), colonModel)
  assert.equal(levelsForSelector('@review', models, roles), colonModel.thinking)
  assert.equal(modelForSelector('@nested', models, roles), undefined)
  assert.equal(modelForSelector('@tier-only', models, roles), undefined)
  assert.equal(modelForSelector('@absent', models, roles), undefined)
  assert.equal(levelsForSelector('@absent', models, roles), THINKING_LEVELS)
})
