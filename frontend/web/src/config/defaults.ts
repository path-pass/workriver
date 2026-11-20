// The page's tunable defaults, in one place: the values are those it always had.
export const KEEP_CODES = 2

// ---- quiet: nothing has moved for a while ----

/** No movement for this long is quiet. ponytail: fixed at 3 days; a setting if users ask. */
export const QUIET = 3 * 864e5

/** The most of a file shown. Markdown takes about 1 s per 256 kB to render (measured with
 *  react-markdown), so it stops there; plain text is cheap. */
export const CAP = { markdown: 256 * 1024, text: 1024 * 1024 }
