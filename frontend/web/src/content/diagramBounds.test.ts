// A mermaid block is weighed from its text, without mermaid, so one too heavy to draw is
// never handed to it: mermaid shares the page's thread and cannot be stopped once started.
import { describe, expect, it } from 'vitest'
import reviewed from './diagramBounds.fixture.mmd?raw'
import { BOUNDS, tooHeavy, weight } from './diagramBounds'

const body = (s: string) => weight(s)

describe("a block's weight", () => {
  it('counts links, one per edge however written', () => {
    expect(body('A --> B')).toBe(1)
    expect(body('A-->B-->C-->D')).toBe(3)
    expect(body('A-->B;B-->C;C-->D')).toBe(3)
    expect(body('A -.-> B\nB ==> C\nC --- D\nD ~~~ E')).toBe(4)
    expect(body('A ->> B: hi\nB -->> A: back')).toBe(2)
    expect(body('C1 <|-- C2\nE1 ||--o{ E2 : places')).toBe(2)
  })

  it('counts every edge an & joins', () => {
    expect(body('A & B & C --> D & E & F')).toBeGreaterThanOrEqual(9)
    expect(body('A & B --> C & D --> E & F')).toBeGreaterThanOrEqual(8)
  })

  it('counts the blocks of a row, a width or a link beside them included', () => {
    expect(body('a b c d e f')).toBe(6)
    expect(body('a:3 b c d e f')).toBe(6)
    expect(body('a b c d e f a-->b')).toBeGreaterThanOrEqual(6)
  })

  it('does not count what is in labels, strings or text after a spaced colon', () => {
    expect(body('A["a -- b -- c"] -->|x -- y -- z| B("p --> q")')).toBe(1)
    expect(body('A[(a [b] --> c)] --> B{{d -- e}}')).toBe(1)
    expect(body('Write docs :done, d1, 2024-01-06, 3d')).toBe(2)
  })

  it('skips comments and blank lines; any other statement weighs one at least', () => {
    expect(body('%% a -- b -- c\n\n   \nA')).toBe(1)
    expect(body('graph TD\nA\nB')).toBe(4)
  })
})

describe('too heavy to draw', () => {
  it("the reviewer's 250 nodes and 499 edges (u15#6 P2-1)", () => {
    expect(weight(reviewed)).toBeGreaterThanOrEqual(499)
    expect(tooHeavy(reviewed)).toBe(true)
  })

  it('more characters than the bound, as a label of 50 001 (u15#6 P3-1)', () => {
    expect(tooHeavy(`graph TD\n  A["${'x'.repeat(50_001)}"]`)).toBe(true)
    const words = (n: number) => `graph TD\n  A["${'word '.repeat(n)}"]`
    expect(tooHeavy(words(1_990))).toBe(false)
    expect(words(1_990).length).toBeLessThanOrEqual(BOUNDS.chars)
    expect(tooHeavy(words(2_000))).toBe(true)
  })

  it('a word longer than the bound: mermaid breaks one in time growing with its square', () => {
    expect(tooHeavy(`graph TD\n  A[" ${'x'.repeat(BOUNDS.word)} "]`)).toBe(false)
    expect(tooHeavy(`graph TD\n  A[" ${'x'.repeat(BOUNDS.word + 1)} "]`)).toBe(true)
  })

  it('just over the weight, not at it', () => {
    const edges = (n: number) => 'graph TD\n' + Array.from({ length: n }, (_, i) => `N${i}-->N${i + 1}`).join('\n')
    expect(weight(edges(10)) - 10).toBe(2)
    expect(tooHeavy(edges(BOUNDS.weight - 2))).toBe(false)
    expect(tooHeavy(edges(BOUNDS.weight - 1))).toBe(true)
  })

  it('not the diagrams agents write: a flowchart and a sequence diagram of a usual size', () => {
    const flow = 'flowchart LR\n' + Array.from({ length: 40 }, (_, i) => `  S${i}[step ${i}] -->|then| S${i + 1}[step ${i + 1}]`).join('\n')
    const seq = 'sequenceDiagram\n  participant U as User\n  participant S as Service\n'
      + Array.from({ length: 30 }, (_, i) => `  U->>S: request ${i}\n  S-->>U: answer ${i}`).join('\n')
    expect(tooHeavy(flow)).toBe(false)
    expect(tooHeavy(seq)).toBe(false)
  })
})
