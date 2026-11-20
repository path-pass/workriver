// Whether a key press lands in a field the user types into, where the page's own keys stay out.

export const isField = (el: EventTarget | null) =>
  el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable)
