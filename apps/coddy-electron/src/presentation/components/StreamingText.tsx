// StreamingText component: reveals text with a smooth typing animation.
// Each character fades in subtly as it appears.

import { useEffect, useRef, useState } from 'react'

interface Props {
  text: string
}

/**
 * Renders streaming text with a subtle per-character fade-in.
 * When new characters arrive, they animate in smoothly.
 */
export function StreamingText({ text }: Props) {
  const [prevLength, setPrevLength] = useState(0)
  const containerRef = useRef<HTMLSpanElement>(null)

  useEffect(() => {
    if (text.length > prevLength) {
      setPrevLength(text.length)
    }
  }, [text.length, prevLength])

  return (
    <span ref={containerRef} className="streaming-text">
      {text.split('').map((char, i) => (
        <span
          key={i}
          className="streaming-char"
          style={{
            animationDelay: `${(i - prevLength + text.length) * 8}ms`,
          }}
        >
          {char === '\n' ? <br /> : char}
        </span>
      ))}
    </span>
  )
}