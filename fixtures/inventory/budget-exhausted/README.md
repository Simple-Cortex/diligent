# budget-exhausted

`max_fetch` is 3. The sitemap lists eight pages of one template and the home page links to two more. The crawl fetches the start page, then prefers pages whose path shape has not been fetched yet (`/about`, `/news/{slug}`, then `/p/{slug}` would come after), and stops at three fetches.

Checks: `partial: true`, `remaining_work` says how many discovered pages were not fetched, the limits include `max_fetch_reached`, unfetched pages cluster by path shape only (`dom_shape: null`) and carry `fetched: false`.
