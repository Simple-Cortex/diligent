# small-site

Twelve HTML pages on one origin, one PDF, one vendor link, a robots.txt that declares the sitemap, and a sitemap that adds pages the links do not reach.

Templates (path shape plus DOM shape): `/news/{slug}` (three pages with the same DOM, one with an `aside` that becomes its own cluster), `/programs/{slug}` (three), `/data/{slug}` (two, data tables). Singletons: home, about, contact (a form).

Checks: sitemap and link sources are merged, `javascript:` and fragment links are ignored, the PDF is a document that is not fetched, the vendor host records the home page as first seen, the structured sample picks home, one representative per multi-page template, the unique pages, the form, the document and the vendor page, and the random set is drawn from the rest with seed = run id.
