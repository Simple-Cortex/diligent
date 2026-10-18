# robots-disallow-all

robots.txt is `User-agent: * / Disallow: /` and declares the sitemap. Fetching robots.txt and the sitemap is allowed; no page is.

Expected: the run still writes both files. Every route from the sitemap is `excluded_by_robots: true` and `fetched: false`, grouped into templates by path shape only (the PDF is a document, not a route). The structured and random samples are empty, `coverage.excluded_by_robots` counts the routes, `discovery_limits` carries `robots_excluded_all`, the sample is `partial`, and the CLI exits 2. Only `/robots.txt` and `/sitemap.xml` are requested.
