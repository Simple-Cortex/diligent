# redirect-off-domain

The home page links to six URLs: a redirect to another registrable domain, a same-origin redirect, a redirect loop, a redirect to `www.example.gov` (same registrable domain, different origin), a timeout and a 404.

Checks: `/go` is `unreachable: off_domain` and `https://evil.example/landing` is never requested; `/moved` is fetched with `final_url` `/new-home`; `/loop` is `too_many_redirects`; `/to-www` is `off_scope_redirect` (the response is not read); `/down` is `timeout`; `/missing` is a fetched route with status 404 that contributes no links. Unreachable routes get no cluster and are never sampled; they only count in `coverage.unreachable`.
