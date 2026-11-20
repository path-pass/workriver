// A stored file, or one the user is attaching.


/** A stored file (a screenshot, a report, a log), or a picture the user is attaching. */
export interface Attachment {
  id: string;
  name: string;
  /** Where the page shows it from: the service's store, or an object URL before upload. */
  url: string;
  /** Its type: pictures, Markdown and plain text preview, anything else downloads. Absent means a picture. */
  type?: string;
}
