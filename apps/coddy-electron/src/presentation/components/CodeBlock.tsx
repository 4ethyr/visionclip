// CodeBlock with language label and copy button.
// Detects code blocks inside markdown text (```lang ... ```) and renders them.

import { useState, useCallback, type CSSProperties } from 'react'
import type { JSX } from 'react'
import { Icon } from './Icon'

// Simple keyword-based syntax highlighting for common languages.
const KEYWORD_MAP: Record<string, string[]> = {
  rust: ['use', 'fn', 'let', 'mut', 'struct', 'enum', 'impl', 'pub', 'async', 'await', 'match', 'return', 'Ok', 'Some', 'None', 'if', 'else', 'for', 'while', 'true', 'false', 'self', 'crate', 'mod', 'type', 'trait', 'where', 'ref', 'move', 'dyn', 'const', 'static', 'unsafe', 'extern', 'macro_rules'],
  typescript: ['export', 'import', 'from', 'const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'type', 'interface', 'class', 'extends', 'implements', 'async', 'await', 'new', 'this', 'super', 'true', 'false', 'null', 'undefined', 'enum', 'typeof', 'keyof', 'infer', 'readonly'],
  javascript: ['export', 'import', 'from', 'const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'class', 'extends', 'new', 'this', 'super', 'true', 'false', 'null', 'undefined', 'async', 'await'],
  python: ['def', 'class', 'import', 'from', 'return', 'if', 'elif', 'else', 'for', 'while', 'True', 'False', 'None', 'async', 'await', 'with', 'as', 'in', 'not', 'and', 'or', 'is', 'raise', 'try', 'except', 'finally', 'yield', 'lambda', 'pass', 'break', 'continue'],
  shell: ['export', 'source', 'echo', 'cd', 'ls', 'cat', 'grep', 'find', 'mkdir', 'rm', 'cp', 'mv', 'chmod', 'chown', 'sudo', 'apt', 'npm', 'cargo', 'npx', 'yarn'],
}

// Map file extensions to language
const EXT_TO_LANG: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  py: 'python',
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
}

const STYLES: Record<string, CSSProperties> = {
  keyword: { color: '#ebb2ff' },
  string: { color: '#7df4ff' },
  comment: { color: '#849495' },
  function: { color: '#00dbe9' },
  number: { color: '#00f0ff' },
}

function highlightLine(line: string, keywords: string[]): JSX.Element {
  const parts: JSX.Element[] = []
  let remaining = line

  while (remaining.length > 0) {
    // Check for strings
    const strMatch = remaining.match(/^("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)/)
    if (strMatch) {
      parts.push(<span key={parts.length} style={STYLES.string}>{strMatch[1]!}</span>)
      remaining = remaining.slice(strMatch[1]!.length)
      continue
    }

    // Check for comments (// or #)
    const commentMatch = remaining.match(/^(\/\/.*|#.*)/)
    if (commentMatch) {
      parts.push(<span key={parts.length} style={STYLES.comment}>{commentMatch[1]!}</span>)
      remaining = remaining.slice(commentMatch[1]!.length)
      continue
    }

    // Check for numbers
    const numMatch = remaining.match(/^(\b\d+\.?\d*\b)/)
    if (numMatch) {
      parts.push(<span key={parts.length} style={STYLES.number}>{numMatch[1]!}</span>)
      remaining = remaining.slice(numMatch[1]!.length)
      continue
    }

    // Check for keywords
    let foundKeyword = false
    for (const kw of keywords) {
      const kwMatch = remaining.match(new RegExp(`^\\b${kw}\\b`))
      if (kwMatch) {
        parts.push(<span key={parts.length} style={STYLES.keyword}>{kwMatch[0]!}</span>)
        remaining = remaining.slice(kwMatch[0]!.length)
        foundKeyword = true
        break
      }
    }
    if (foundKeyword) continue

    // Check for function calls (identifiers followed by `(`)
    const funcMatch = remaining.match(/^(\w+)(\()/)
    if (funcMatch) {
      parts.push(<span key={parts.length} style={STYLES.function}>{funcMatch[1]!}</span>)
      remaining = remaining.slice(funcMatch[1]!.length)
      continue
    }

    // Consume one character
    parts.push(<span key={parts.length}>{remaining[0]}</span>)
    remaining = remaining.slice(1)
  }

  return <>{parts}</>
}

// ─── Public API ───────────────────────────────────────────────────────────

interface CodeBlockProps {
  code: string
  language?: string
}

export function CodeBlock({ code, language }: CodeBlockProps) {
  const [copied, setCopied] = useState(false)

  const lang = language
    ? (EXT_TO_LANG[language.toLowerCase()] ?? language.toLowerCase())
    : ''
  const keywords = lang ? (KEYWORD_MAP[lang] ?? []) : []
  const lines = code.split('\n')

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(code)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }, [code])

  return (
    <div className="code-block mt-3 overflow-hidden rounded-lg">
      <div className="flex items-center justify-between border-b border-white/10 bg-surface-container-highest/80 px-4 py-2">
        <span className="font-mono text-xs uppercase tracking-[0.18em] text-on-surface-variant">
          {lang || 'code'}
        </span>
        <button
          type="button"
          onClick={handleCopy}
          className="flex items-center gap-1.5 font-mono text-xs text-on-surface-variant transition-colors hover:text-primary"
        >
          <Icon name="copy" className="h-3.5 w-3.5" />
          {copied ? 'Copied!' : 'Copy'}
        </button>
      </div>

      <div className="p-4 overflow-x-auto">
        <pre className="font-mono text-sm leading-relaxed text-on-surface/95">
          {lines.map((line, i) => (
            <div key={i}>
              {highlightLine(line, keywords)}
              {'\n'}
            </div>
          ))}
        </pre>
      </div>
    </div>
  )
}

// ─── Markdown parsing helper ──────────────────────────────────────────────

interface MarkdownSegment {
  type: 'text' | 'code'
  content: string
  language?: string
}

/**
 * Splits a markdown string into segments: text and ```code blocks```.
 * Used by MessageBubble to render code blocks with syntax highlighting.
 */
export function parseMarkdown(text: string): MarkdownSegment[] {
  const segments: MarkdownSegment[] = []
  const regex = /```(\w+)?\s*\n([\s\S]*?)```/g
  let lastIndex = 0
  let match: RegExpExecArray | null

  while ((match = regex.exec(text)) !== null) {
    // Text before this code block
    if (match.index > lastIndex) {
      segments.push({ type: 'text', content: text.slice(lastIndex, match.index) })
    }

    segments.push({
      type: 'code',
      content: match[2]!.trim(),
      language: match[1] ?? undefined,
    })

    lastIndex = regex.lastIndex
  }

  // Remaining text
  if (lastIndex < text.length) {
    segments.push({ type: 'text', content: text.slice(lastIndex) })
  }

  return segments
}
