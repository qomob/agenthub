import { describe, expect, it } from 'vitest'

import { parseFrontmatter, parseSkillMd } from '../markdown'

describe('parseSkillMd', () => {
  const sample = [
    '---',
    'name: demo-skill',
    'description: A demo',
    '---',
    '',
    '# Demo',
    '',
    'Some **plain** text.',
    '',
    '```ts',
    "const html = '<script>alert(1)</script>'",
    '```',
    '',
    '- first',
    '- second',
    '',
    '> a quote',
  ].join('\n')

  it('extracts frontmatter, headings, code, lists and quotes in order', () => {
    const blocks = parseSkillMd(sample)
    expect(blocks[0]).toEqual({
      kind: 'frontmatter',
      text: 'name: demo-skill\ndescription: A demo',
    })
    expect(blocks[1]).toEqual({ kind: 'heading', level: 1, text: 'Demo' })
    expect(blocks[2]).toEqual({ kind: 'text', text: 'Some **plain** text.' })
    expect(blocks[3]).toEqual({
      kind: 'code',
      lang: 'ts',
      code: "const html = '<script>alert(1)</script>'",
    })
    expect(blocks[4]).toEqual({ kind: 'list', ordered: false, items: ['first', 'second'] })
    expect(blocks[5]).toEqual({ kind: 'quote', text: 'a quote' })
  })

  it('keeps raw HTML as inert text rather than markup', () => {
    const blocks = parseSkillMd('<img src=x onerror=alert(1)>')
    expect(blocks).toEqual([{ kind: 'text', text: '<img src=x onerror=alert(1)>' }])
  })

  it('parses ordered lists and unclosed fences', () => {
    const blocks = parseSkillMd('1. one\n2. two\n\n```\nunclosed')
    expect(blocks[0]).toEqual({ kind: 'list', ordered: true, items: ['one', 'two'] })
    expect(blocks[1]).toEqual({ kind: 'code', lang: '', code: 'unclosed' })
  })
})

describe('parseFrontmatter', () => {
  it('extracts simple key/value pairs and strips quotes', () => {
    expect(parseFrontmatter('name: "demo"\nversion: 1.2.3\nnot a pair')).toEqual([
      ['name', 'demo'],
      ['version', '1.2.3'],
    ])
  })
})
