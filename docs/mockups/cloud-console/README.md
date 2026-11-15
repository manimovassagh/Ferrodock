# Cloud console mockups

Mockups of the emulator's web console, after the patterns and tokens of
Cloudscape, AWS's open-source design system (Apache-2.0). They're the
plan for [#408](https://github.com/manimovassagh/Ferrodock/issues/408).

| File | Screen |
| --- | --- |
| `1-buckets.png` | S3's buckets: a table with row selection, an Actions menu, paging, and a notice |
| `2-create-bucket.png` | Create bucket: a full page, the name checked as it's typed |
| `3-create-queue.png` | Create queue: Standard or FIFO, configuration, dead-letter queue |
| `4-delete-modal.png` | Delete: a dialog in the page, typing the name to confirm |
| `5-queue.png`, `6-queue-dark.png` | A queue: details, tabs, and its messages, light and dark |

`make.py` writes the pages from one frame and `shell.css`; open them in a
browser, or render them as PNGs with Playwright. The data in them is made
up. They use ferrocloud's own name, logo and icons: no AWS logo, service
icons or typeface.
