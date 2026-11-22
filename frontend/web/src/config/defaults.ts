// The page's tunable defaults, in one place: the values are those it always had.

/** How many units' file texts are kept (code/load.ts): going back and forth between two stays instant. */
export const KEEP_CODES = 2

// ---- quiet: nothing has moved for a while ----

/** No movement for this long is quiet. ponytail: fixed at 3 days; a setting if users ask. */
export const QUIET = 3 * 864e5

/** The most of a file shown. Markdown takes about 1 s per 256 kB to render (measured with
 *  react-markdown), so it stops there; plain text is cheap. */
export const CAP = { markdown: 256 * 1024, text: 1024 * 1024 }

/** The widths a user can drag, in px: the default, and the least and most it can be. The code
 *  layer's file tree is at most half the layer's width, and never less than its least. */
export const RAIL_WIDTH = { def: 248, min: 200, max: 480 }
export const SIDE_WIDTH = { def: 340, min: 280, max: 720 }
export const CODE_TREE_WIDTH = { def: 280, min: 220 }
/** The least height the Activity graph's canvas can be dragged to, in px; the most is the
 *  window's height. With none set, the canvas takes the graph's own height (Activity.tsx). */
export const FLOW_HEIGHT = { min: 160 }

/** A hidden tab reads the workspace at most this often, in ms, when the service says it
 *  changed, and only while the sound or notifications is on (workspace/source.ts). */
export const HIDDEN_READ = 5000
