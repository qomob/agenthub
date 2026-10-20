/**
 * A deliberately tiny, safe SKILL.md block parser.
 *
 * Rendering is done with text interpolation only (never `v-html`), so untrusted
 * markdown/HTML from imported skills can never become live markup.
 */
export type SkillMdBlock =
  | { kind: 'frontmatter'; text: string }
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'code'; lang: string; code: string }
  | { kind: 'list'; ordered: boolean; items: string[] }
  | { kind: 'quote'; text: string }
  | { kind: 'text'; text: string }

const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---[ \t]*(?:\r?\n|$)/

export function parseSkillMd(markdown: string): SkillMdBlock[] {
  const normalized = (markdown ?? '').replace(/\r\n/g, '\n')
  const blocks: SkillMdBlock[] = []
  let rest = normalized

  const frontmatter = FRONTMATTER.exec(normalized)
  if (frontmatter) {
    blocks.push({ kind: 'frontmatter', text: frontmatter[1] })
    rest = normalized.slice(frontmatter[0].length)
  }

  const lines = rest.split('\n')
  let index = 0
  let paragraph: string[] = []
  let listItems: string[] = []
  let listOrdered = false
  let quoteLines: string[] = []

  const flushParagraph = () => {
    if (paragraph.length > 0) {
      blocks.push({ kind: 'text', text: paragraph.join('\n').trim() })
      paragraph = []
    }
  }
  const flushList = () => {
    if (listItems.length > 0) {
      blocks.push({ kind: 'list', ordered: listOrdered, items: listItems })
      listItems = []
    }
  }
  const flushQuote = () => {
    if (quoteLines.length > 0) {
      blocks.push({ kind: 'quote', text: quoteLines.join('\n').trim() })
      quoteLines = []
    }
  }
  const flushAll = () => {
    flushParagraph()
    flushList()
    flushQuote()
  }

  while (index < lines.length) {
    const line = lines[index]

    if (line.startsWith('```')) {
      flushAll()
      const lang = line.slice(3).trim()
      const code: string[] = []
      index += 1
      while (index < lines.length && !lines[index].startsWith('```')) {
        code.push(lines[index])
        index += 1
      }
      if (index < lines.length) index += 1
      blocks.push({ kind: 'code', lang, code: code.join('\n') })
      continue
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(line)
    if (heading) {
      flushAll()
      blocks.push({
        kind: 'heading',
        level: heading[1].length,
        text: heading[2].trim(),
      })
      index += 1
      continue
    }

    const unordered = /^\s*[-*+]\s+(.*)$/.exec(line)
    const ordered = /^\s*\d+[.)]\s+(.*)$/.exec(line)
    if (unordered || ordered) {
      flushParagraph()
      flushQuote()
      const isOrdered = Boolean(ordered)
      if (listItems.length > 0 && isOrdered !== listOrdered) flushList()
      listOrdered = isOrdered
      listItems.push((unordered ? unordered[1] : ordered![1]).trim())
      index += 1
      continue
    }

    if (/^\s*>\s?/.test(line)) {
      flushParagraph()
      flushList()
      quoteLines.push(line.replace(/^\s*>\s?/, ''))
      index += 1
      continue
    }

    if (/^\s*$/.test(line)) {
      flushAll()
      index += 1
      continue
    }

    flushList()
    flushQuote()
    paragraph.push(line)
    index += 1
  }

  flushAll()
  return blocks
}

/** Extract `key: value` pairs from the YAML frontmatter block (display only). */
export function parseFrontmatter(text: string): Array<[string, string]> {
  const out: Array<[string, string]> = []
  for (const line of text.split('\n')) {
    const match = /^([A-Za-z0-9_.-]+)\s*:\s*(.*)$/.exec(line.trim())
    if (!match) continue
    out.push([match[1], match[2].replace(/^["']|["']$/g, '')])
  }
  return out
}
