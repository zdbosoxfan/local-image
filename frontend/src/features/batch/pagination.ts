/** Only the visible page creates rows and preview elements. Selection belongs
 * to the controller and always covers the entire collection or reviewed queue. */
export const BATCH_PAGE_SIZE = 50;

export function batchPage(total: number, requestedPage: number) {
  const pages = Math.max(1, Math.ceil(total / BATCH_PAGE_SIZE));
  const page = Math.min(pages - 1, Math.max(0, Number.isFinite(requestedPage) ? Math.floor(requestedPage) : 0));
  const start = page * BATCH_PAGE_SIZE, end = Math.min(total, start + BATCH_PAGE_SIZE);
  return {page, pages, total, start, end, from: total ? start + 1 : 0, to: end};
}
