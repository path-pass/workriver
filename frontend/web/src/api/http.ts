// Requests to the service: a POST, a read, the stream of changes, and what a refusal of this
// page as the user does.
/** Sent on the window when the service says this page is not the user (any more). */
export const SIGNED_OUT = 'workriver:signed-out'

export async function post(path: string, body: unknown) {
  const res = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`)
}

/** A POST that answers JSON; a refusal is thrown with the service's words. */
export async function call<T>(path: string, body: unknown, { includeStatus = false } = {}): Promise<T> {
  const res = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) {
    const words = await res.text()
    throw new Error(includeStatus ? `${path}: ${res.status} ${words}` : words)
  }
  return res.status === 204 ? (undefined as T) : res.json()
}

/**
 * A GET that answers JSON (the workspace, a unit's code); a refusal is thrown as its status
 * and the service's words. It sends no SIGNED_OUT on a 401, as these reads never did (the
 * POSTs above and a run's read do): sending it here would be a change of behaviour.
 */
export async function read<T>(path: string): Promise<T> {
  const res = await fetch(path)
  if (!res.ok) throw new Error(`${res.status} ${await res.text()}`)
  return res.json()
}

/** The service's stream that says the workspace changed (`changed`); it reconnects by itself. */
export const changes = () => new EventSource('/api/events')
