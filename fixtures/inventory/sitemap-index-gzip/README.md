# sitemap-index-gzip

`robots.txt` declares a sitemap index and a second `Sitemap:` line that points at a private host. The index lists a gzip urlset, a plain urlset, itself (a loop) and a child on another origin. The plain urlset has an off-origin URL, an entry whose `loc` is not a URL, and an entry without `loc`.

Checks: the gzip file is decoded by magic bytes, the loop is visited once, the private-host and other-origin sitemaps are never requested, the off-origin entries are counted as `off_scope`, invalid entries are noted, and seven pages come from `sitemap` sources labelled `robots_sitemap`.
