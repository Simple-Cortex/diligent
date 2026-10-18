# robots-disallow-section

The `*` group disallows `/private/`, re-allows `/private/public-faq` (the longer pattern wins), disallows `*.json` at the end of a path, and asks for a 5 second `Crawl-delay`. A `badbot` group disallowing everything must not apply.

`options.json`: `/private/records` and `/data.json` are listed with `excluded_by_robots: true` and never requested; the limit line reads `robots_excluded: 2 urls`; requests are 5 seconds apart on the manual clock.

`options-ignore.json` (`ignore_robots: true`): every page is fetched and the limit line reads `robots_overridden_by_operator`; the crawl delay is still honored.
