// Requests to the service: a POST, and what a refusal of this page as the user does.
/** Sent on the window when the service says this page is not the user (any more). */
export const SIGNED_OUT = 'workriver:signed-out'

export async function post(path: string, body: unknown) {
  const res = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) throw new Error(`${path}: ${res.status} ${await res.text()}`)
}

/** A POST that answers JSON; a refusal is thrown with the service's words. */
export async function call<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
  if (res.status === 401) window.dispatchEvent(new Event(SIGNED_OUT))
  if (!res.ok) throw new Error(await res.text())
  return res.status === 204 ? (undefined as T) : res.json()
}
