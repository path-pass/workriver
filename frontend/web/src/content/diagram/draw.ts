// Draws a mermaid block of Markdown an agent wrote. Mermaid runs in a frame of its own, made
// the first time a block is shown, whose page (FRAME) may run only its own script and fetch
// nothing: what a block makes mermaid do while drawing stays there. Its script
// (frame.ts, with mermaid) is built on its own, as a worker's is, so the app's bundle
// shares nothing with it. The page gets back the SVG text and shows it only as an image
// (Diagram.tsx). One diagram is drawn at a time, with a pause between, so many or large ones
// do not hold the page; one no longer shown is skipped when its turn comes.
// Its own file: the seam the component's tests (Diagram.test.tsx) fake.
import script from './frame.ts?worker&url'

/** What mermaid drew, and its size in CSS pixels. */
export type Picture = { svg: string; width: number; height: number }

export class MermaidError extends Error {}

const WAIT = 20_000
let turn: Promise<unknown> = Promise.resolve()

/** Runs `job` after every job queued before it has settled, unless `signal` was aborted by then. */
export function inTurn<T>(job: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  const run = turn.then(() => {
    signal?.throwIfAborted()
    return job()
  })
  turn = run.catch(() => undefined).then(() => new Promise(r => setTimeout(r)))
  return run
}

/** The size the root `<svg>` declares in its viewBox; none, and it is not a diagram. */
export function size(svg: string): { width: number; height: number } | undefined {
  const box = /^<svg\b[^>]*?\sviewBox="[-\d.e]+[\s,]+[-\d.e]+[\s,]+([\d.e]+)[\s,]+([\d.e]+)"/.exec(svg)
  if (!box) return undefined
  const [width, height] = [Number(box[1]), Number(box[2])].map(Math.ceil)
  return width > 0 && height > 0 ? { width, height } : undefined
}

/** The frame's page: only its script runs, and nothing is fetched (no image, font or request). */
export const FRAME = (src: string) => '<!doctype html><html><head><meta charset="utf-8">'
  + `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self'; style-src 'unsafe-inline'">`
  + `</head><body><script type="module" src="${src}"></script></body></html>`

let frame: Promise<HTMLIFrameElement> | undefined

/** The frame, loaded and ready; a frame that does not get ready is dropped, to be made again. */
function open(): Promise<HTMLIFrameElement> {
  frame ??= new Promise<HTMLIFrameElement>((ok, fail) => {
    const f = document.createElement('iframe')
    f.srcdoc = FRAME(new URL(script, location.href).href)
    f.dataset.diagrams = ''
    f.tabIndex = -1
    f.setAttribute('aria-hidden', 'true')
    // Laid out (mermaid measures its text), never seen.
    f.style.cssText = 'position:fixed;left:-10000px;top:0;width:1200px;height:900px;border:0;visibility:hidden'
    const timer = setTimeout(() => { removeEventListener('message', ready); fail(new Error('the diagram frame did not load')) }, WAIT)
    const ready = (e: MessageEvent) => {
      if (e.source !== f.contentWindow || e.origin !== origin || !e.data?.ready) return
      clearTimeout(timer); removeEventListener('message', ready); ok(f)
    }
    addEventListener('message', ready)
    document.body.append(f)
  }).catch(e => { drop(); throw e })
  return frame
}

function drop() {
  document.querySelectorAll('iframe[data-diagrams]').forEach(f => f.remove())
  frame = undefined
}

let next = 0

function ask(f: HTMLIFrameElement, code: string, dark: boolean): Promise<string> {
  const id = next++
  return new Promise((ok, fail) => {
    const timer = setTimeout(() => { end(); drop(); fail(new Error('the diagram took too long')) }, WAIT)
    const end = () => { clearTimeout(timer); removeEventListener('message', answer) }
    const answer = (e: MessageEvent) => {
      if (e.source !== f.contentWindow || e.origin !== origin || e.data?.id !== id) return
      end()
      if (typeof e.data.svg === 'string') ok(e.data.svg)
      else if (typeof e.data.error === 'string') fail(new MermaidError(e.data.error))
      else fail(new Error('mermaid could not draw it'))
    }
    addEventListener('message', answer)
    f.contentWindow?.postMessage({ id, code, dark }, origin)
  })
}

/** Draws `code` in the page's theme; rejects when mermaid cannot, or the frame fails. */
export function drawDiagram(code: string, dark: boolean, signal?: AbortSignal): Promise<Picture> {
  return inTurn(async () => {
    const svg = await ask(await open(), code, dark)
    const box = size(svg)
    if (!box) throw new Error('not a diagram')
    return { svg, ...box }
  }, signal)
}
