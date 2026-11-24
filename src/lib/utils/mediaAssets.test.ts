import test from 'node:test'
import assert from 'node:assert/strict'
import { markdownImageSrc, resolveMarkdownImagePath } from './mediaAssets'

const local = '/repo/rust/Axome'
const remote = 'axome://server/repo'
const home = '/home/toph'

test('local desktop chat resolves explicit host paths and keeps relative images project-relative', () => {
  const cases = [
    [`${local}/src-tauri/icons/icon.png`, `${local}/src-tauri/icons/icon.png`],
    ['/usr/share/pixmaps/example.png', '/usr/share/pixmaps/example.png'],
    ['/tmp/icon%20name.png?x#y', '/tmp/icon%20name.png?x#y'],
    ['file:///tmp/icon.png', '/tmp/icon.png'],
    ['FiLe://LOCALHOST/tmp/icon.png', '/tmp/icon.png'],
    ['file://other-host/tmp/icon.png', null],
    ['file:/tmp/icon.png', null],
    ['file:icon.png', null],
    ['~/icon.png', `${home}/icon.png`],
    ['images/icon.png', 'images/icon.png'],
    ['../outside.png', null],
    ['', null]
  ] as const
  for (const [href, expected] of cases) {
    assert.equal(resolveMarkdownImagePath(href, local, undefined, true, `${home}/`), expected, href)
  }
  for (const invalidHome of [undefined, '', 'relative']) {
    assert.equal(resolveMarkdownImagePath('~/icon.png', local, null, true, invalidHome), null)
  }
  for (const folder of [undefined, '']) {
    assert.equal(resolveMarkdownImagePath('/images/icon.png', folder, null, true, home), null)
  }
})

test('web and remote chats keep project-root paths and reject host-only references', () => {
  for (const [folder, native] of [
    [local, false],
    [remote, true]
  ] as const) {
    assert.equal(resolveMarkdownImagePath('/images/icon.png', folder, null, native, home), 'images/icon.png')
    for (const href of ['~/icon.png', 'file:///tmp/icon.png', 'file://localhost/tmp/icon.png']) {
      assert.equal(resolveMarkdownImagePath(href, folder, null, native, home), null, href)
    }
  }
})

test('file previews retain document and project-root resolution on every platform', () => {
  const cases = [
    ['docs/README.md', 'images/icon.png', 'docs/images/icon.png'],
    ['docs/README.md', '../icon.png', 'icon.png'],
    ['README.md', '/static/favicon.png', 'static/favicon.png'],
    ['', '/static/favicon.png', 'static/favicon.png'],
    ['README.md', 'file:///tmp/icon.png', null],
    ['README.md', '~/icon.png', null]
  ] as const
  for (const [folder, native] of [
    [local, true],
    [local, false],
    [remote, true]
  ] as const) {
    for (const [file, href, expected] of cases) {
      assert.equal(resolveMarkdownImagePath(href, folder, file, native, home), expected, `${folder}: ${href}`)
    }
  }
})

test('markdownImageSrc classifies external and local links before platform resolution', async () => {
  for (const href of [null, undefined, '', '   ']) {
    assert.deepEqual(await markdownImageSrc(href), { url: null, local: true })
  }
  for (const [href, expected] of [
    ['//cdn.example.com/a.png', '//cdn.example.com/a.png'],
    ['  //cdn.example.com/a.png  ', '//cdn.example.com/a.png'],
    ['https://example.com/a.png', 'https://example.com/a.png'],
    ['data:image/png;base64,abc', 'data:image/png;base64,abc']
  ] as const) {
    assert.deepEqual(await markdownImageSrc(href), { url: expected, local: false })
  }
  for (const href of ['file:///tmp/a.png', './local.png', '/abs/a.png']) {
    assert.deepEqual(await markdownImageSrc(href), { url: null, local: true })
  }
})
