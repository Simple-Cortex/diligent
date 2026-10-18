# time-budget

robots.txt asks for a 10 second `Crawl-delay` and the site has a home page plus twelve pages in the sitemap. `options.json` sets `max_time_ms: 60000`.

On the manual clock the requests start at 0, 10, 20, 30, 40 and 50 seconds: robots.txt, `sitemap.xml`, the home page and three more pages. The seventh would start at 60 seconds, so the run stops. Expected: `partial: true`, `remaining_work` "time budget of 60 s reached after 6 fetches", a `time_budget_reached` limit, a `request_interval` limit naming the Crawl-delay, and the exit code of the CLI would be 2.
